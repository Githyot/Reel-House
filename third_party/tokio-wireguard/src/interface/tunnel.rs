use std::{
    io,
    net::{IpAddr, Ipv6Addr, SocketAddr, SocketAddrV6},
    sync::Arc,
};

use boringtun::{
    noise::{
        rate_limiter::RateLimiter, HandshakeResponse, Packet, PacketCookieReply, PacketData, Tunn,
        TunnResult,
    },
    x25519::{PublicKey, StaticSecret},
};
use rand::SeedableRng;
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
use tokio::net::UdpSocket;

use super::random::Lfsr24;

pub struct Tunnel {
    address: crate::config::Address,
    keypair: (StaticSecret, PublicKey),
    peers: Vec<Peer>,
    rng: Lfsr24,
    rate_limiter: Arc<RateLimiter>,
    buffers: (Vec<u8>, Vec<u8>),
    socket: UdpSocket,
}

pub trait IntoPacket<'a> {
    fn into_packet(self, buffer: &'a [u8]) -> &'a [u8];
}

impl Tunnel {
    pub fn new(
        config: &crate::config::Interface,
        peer_configs: impl IntoIterator<Item = crate::config::Peer>,
    ) -> Result<Self, io::Error> {
        let private_key = config.private_key.clone();
        let public_key = PublicKey::from(&private_key);

        let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
        socket.set_nonblocking(true)?;
        socket.set_only_v6(false)?;
        #[cfg(not(windows))]
        socket.set_reuse_address(true)?;

        let address: SockAddr =
            SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, config.listen_port.unwrap_or(0), 0, 0).into();
        socket.bind(&address)?;

        let socket = UdpSocket::from_std(socket.into())?;
        let mut tunnel = Tunnel {
            address: config.address,
            keypair: (private_key, public_key),
            peers: Vec::new(),
            rng: Lfsr24::from_entropy(),
            rate_limiter: Arc::new(RateLimiter::new(&public_key, 64)),
            buffers: (vec![0; u16::MAX as usize], vec![0; u16::MAX as usize]),
            socket,
        };

        for peer_config in peer_configs {
            tunnel.add_peer(peer_config)?;
        }
        Ok(tunnel)
    }

    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    pub async fn recv(&mut self) -> Option<&[u8]> {
        let Self {
            address,
            keypair: (private, public),
            peers,
            buffers,
            socket,
            ..
        } = self;

        let (len, source) = socket.try_recv_from(&mut buffers.0).ok()?;
        // the three reserved header bytes may carry a value (Cloudflare WARP): boringtun wants them zero
        if len >= 4 {
            buffers.0[1..4].fill(0);
        }
        let packet = Tunn::parse_incoming_packet(&buffers.0[..len]).ok()?;

        let peer = match packet {
            Packet::HandshakeInit(packet) => {
                let handshake =
                    boringtun::noise::handshake::parse_handshake_anon(private, public, &packet)
                        .ok()?;
                peers.iter_mut().find(|p| {
                    p.active && p.config.public_key.as_bytes() == &handshake.peer_static_public
                })?
            }

            Packet::HandshakeResponse(HandshakeResponse { receiver_idx, .. })
            | Packet::PacketCookieReply(PacketCookieReply { receiver_idx, .. })
            | Packet::PacketData(PacketData { receiver_idx, .. }) => peers
                .iter_mut()
                .find(|p| p.active && p.index == receiver_idx >> 8)?,
        };

        let packet =
            match peer
                .tunnel
                .decapsulate(Some(source.ip()), &buffers.0[..len], &mut buffers.1)
            {
                TunnResult::Err(..) => return None,
                TunnResult::Done => None,
                TunnResult::WriteToNetwork(mut datagram) => loop {
                    mark(datagram, peer.config.reserved);
                    socket.send_to(datagram, source).await.ok();
                    match peer.tunnel.decapsulate(None, &[], &mut buffers.0) {
                        TunnResult::WriteToNetwork(d) => datagram = d,
                        _ => break None,
                    }
                },
                TunnResult::WriteToTunnelV4(p, a) if peer.is_allowed_ip(a) => Some(p),
                TunnResult::WriteToTunnelV6(p, a) if peer.is_allowed_ip(a) => Some(p),
                _ => None,
            };

        peer.config.endpoint = Some(source);
        match packet {
            Some(packet_out) => {
                let len = packet_out.len();
                buffers.0[..len].copy_from_slice(packet_out);

                let address = *address;
                self.send(len, |destination| {
                    address.networks().any(|net| net.contains(&destination))
                })
                .await
            }
            None => None,
        }
    }

    pub async fn send<'s>(
        &'s mut self,
        packet: impl IntoPacket<'s>,
        relay: impl FnOnce(IpAddr) -> bool,
    ) -> Option<&'s [u8]> {
        let Self {
            address,
            peers,
            buffers,
            socket,
            ..
        } = self;

        let packet = packet.into_packet(&buffers.0);
        let destination = Tunn::dst_address(packet)?;

        if address.addresses().any(|ip| ip == destination) {
            return Some(packet);
        }
        if !relay(destination) {
            return None;
        }

        let peer = peers.iter_mut().find(|p| {
            p.active
                && p.config
                    .allowed_ips
                    .iter()
                    .any(|net| net.contains(&destination))
        })?;

        let endpoint = mapped(peer.config.endpoint?);
        let reserved = peer.config.reserved;
        let packet = peer.tunnel.encapsulate(packet, &mut buffers.1);
        if let TunnResult::WriteToNetwork(packet) = packet {
            mark(packet, reserved);
            socket.send_to(packet, endpoint).await.ok();
        }

        None
    }

    pub async fn update_timers(&mut self) {
        let Self {
            peers,
            buffers,
            socket,
            rate_limiter,
            ..
        } = self;

        // the shared limiter counts handshakes; boringtun only resets a limiter it made itself (Reelhouse fix:
        // without this, after 64 handshakes every reply was treated as "under load" and refused)
        rate_limiter.reset_count();
        for peer in peers.iter_mut() {
            let endpoint = match (peer.active, peer.config.endpoint) {
                // the socket is IPv6 with IPv4 mapped in: an IPv4 address must be mapped (Reelhouse fix: handshake
                // retries and keepalives to an IPv4 server never left)
                (true, Some(endpoint)) => mapped(endpoint),
                _ => continue,
            };
            let reserved = peer.config.reserved;
            let packet = peer.tunnel.update_timers(&mut buffers.0);
            if let TunnResult::WriteToNetwork(packet) = packet {
                mark(packet, reserved);
                socket.send_to(packet, endpoint).await.ok();
            }
        }
    }

    pub fn add_peer(&mut self, config: crate::config::Peer) -> Result<(), io::Error> {
        let Self {
            keypair: (private, _),
            peers,
            rng,
            rate_limiter,
            ..
        } = self;

        let index = rng.next();
        let peer = Tunn::new(
            private.clone(),
            config.public_key,
            config.preshared_key,
            config.persistent_keepalive,
            index,
            Some(rate_limiter.clone()),
        );
        let peer = Peer {
            config,
            tunnel: peer,
            index,
            active: true,
        };

        let slot = peers.iter_mut().find(|p| !p.active);
        if let Some(slot) = slot {
            *slot = peer;
        } else {
            peers.push(peer);
        }
        Ok(())
    }

    pub fn remove_peer(&mut self, key: &PublicKey) -> bool {
        match self.peers.iter_mut().find(|p| &p.config.public_key == key) {
            Some(peer) => {
                peer.active = false;
                true
            }
            None => false,
        }
    }
}

/// An IPv4 address as the dual-stack IPv6 socket needs it.
fn mapped(addr: SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V4(a) => SocketAddr::V6(SocketAddrV6::new(a.ip().to_ipv6_mapped(), a.port(), 0, 0)),
        e => e,
    }
}

/// The reserved header bytes of an outgoing message.
fn mark(packet: &mut [u8], reserved: [u8; 3]) {
    if packet.len() >= 4 && reserved != [0, 0, 0] {
        packet[1..4].copy_from_slice(&reserved);
    }
}

impl<'a> IntoPacket<'a> for &'a [u8] {
    fn into_packet(self, _: &'a [u8]) -> &'a [u8] {
        self
    }
}

impl<'a> IntoPacket<'a> for usize {
    fn into_packet(self, buffer: &'a [u8]) -> &'a [u8] {
        &buffer[..self]
    }
}

struct Peer {
    config: crate::config::Peer,
    tunnel: Tunn,
    index: u32,
    active: bool,
}

impl Peer {
    fn is_allowed_ip(&self, address: impl Into<IpAddr>) -> bool {
        let address = address.into();
        self.config
            .allowed_ips
            .iter()
            .any(|net| net.contains(&address))
    }
}
