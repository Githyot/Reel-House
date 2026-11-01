//! A small read-only SQLite reader: enough to open the collection inside an Anki package (`.apkg`).
//! It walks the table b-trees of a database file held in memory, follows overflow pages, and hands back every row of a table.
//! Standard library only. Not supported: write-ahead logs, non-UTF-8 text, encrypted files.

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    pub fn int(&self) -> i64 {
        match self {
            Value::Int(i) => *i,
            Value::Real(f) => *f as i64,
            Value::Text(t) => t.trim().parse().unwrap_or(0),
            _ => 0,
        }
    }
    pub fn text(&self) -> String {
        match self {
            Value::Text(t) => t.clone(),
            Value::Int(i) => i.to_string(),
            Value::Real(f) => f.to_string(),
            Value::Blob(b) => String::from_utf8_lossy(b).to_string(),
            Value::Null => String::new(),
        }
    }
}

pub struct Db<'a> {
    d: &'a [u8],
    page: usize,
    usable: usize,
}

fn be16(d: &[u8], p: usize) -> usize {
    d.get(p..p + 2).map(|b| u16::from_be_bytes([b[0], b[1]]) as usize).unwrap_or(0)
}
fn be32(d: &[u8], p: usize) -> usize {
    d.get(p..p + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0)
}

/// A varint: (value, bytes used).
fn varint(d: &[u8], p: usize) -> (u64, usize) {
    let mut v: u64 = 0;
    for i in 0..9 {
        let Some(&b) = d.get(p + i) else { return (v, i) };
        if i == 8 {
            return ((v << 8) | b as u64, 9);
        }
        v = (v << 7) | (b & 0x7f) as u64;
        if b & 0x80 == 0 {
            return (v, i + 1);
        }
    }
    (v, 9)
}

impl<'a> Db<'a> {
    pub fn open(d: &'a [u8]) -> Result<Db<'a>, String> {
        if d.len() < 100 || &d[0..15] != b"SQLite format 3" {
            return Err("This is not an SQLite database.".into());
        }
        let mut page = be16(d, 16);
        if page == 1 {
            page = 65536;
        }
        if page < 512 || !page.is_power_of_two() {
            return Err("The database has an unusual page size.".into());
        }
        let enc = be32(d, 56);
        if enc != 1 && enc != 0 {
            return Err("The database text is not UTF-8.".into());
        }
        let reserved = d[20] as usize;
        Ok(Db { d, page, usable: page - reserved })
    }

    fn page_at(&self, n: usize) -> Option<&'a [u8]> {
        if n == 0 {
            return None;
        }
        self.d.get((n - 1) * self.page..n * self.page)
    }

    /// The root page of a table, found in `sqlite_master` (page 1).
    pub fn root_of(&self, table: &str) -> Option<usize> {
        let mut found = None;
        self.walk(1, &mut |_, rec| {
            if rec.len() >= 4 && rec[0].text() == "table" && rec[1].text() == table {
                found = Some(rec[3].int() as usize);
            }
        });
        found
    }

    /// Every row of a table as (rowid, columns). An INTEGER PRIMARY KEY column reads as Null: use the rowid.
    pub fn rows(&self, table: &str) -> Vec<(i64, Vec<Value>)> {
        let mut out = Vec::new();
        if let Some(root) = self.root_of(table) {
            self.walk(root, &mut |id, rec| out.push((id, rec)));
        }
        out
    }

    fn walk(&self, root: usize, f: &mut dyn FnMut(i64, Vec<Value>)) {
        let mut stack = vec![root];
        let mut guard = 0usize;
        while let Some(n) = stack.pop() {
            guard += 1;
            if guard > 5_000_000 {
                return;
            }
            let Some(pg) = self.page_at(n) else { continue };
            let base = if n == 1 { 100 } else { 0 };
            let kind = pg[base];
            let cells = be16(pg, base + 3);
            match kind {
                0x05 => {
                    let right = be32(pg, base + 8);
                    let mut kids = Vec::new();
                    for i in 0..cells {
                        let p = be16(pg, base + 12 + i * 2);
                        kids.push(be32(pg, p));
                    }
                    kids.push(right);
                    for k in kids.into_iter().rev() {
                        stack.push(k);
                    }
                }
                0x0d => {
                    for i in 0..cells {
                        let p = be16(pg, base + 8 + i * 2);
                        let (plen, a) = varint(pg, p);
                        let (rowid, b) = varint(pg, p + a);
                        let start = p + a + b;
                        let payload = self.payload(pg, start, plen as usize);
                        f(rowid as i64, Self::record(&payload));
                    }
                }
                _ => {}
            }
        }
    }

    fn payload(&self, pg: &[u8], start: usize, total: usize) -> Vec<u8> {
        let u = self.usable;
        let max_local = u - 35;
        if total <= max_local {
            return pg.get(start..start + total).map(|b| b.to_vec()).unwrap_or_default();
        }
        let min_local = ((u - 12) * 32 / 255) - 23;
        let k = min_local + (total - min_local) % (u - 4);
        let local = if k <= max_local { k } else { min_local };
        let mut out = pg.get(start..start + local).map(|b| b.to_vec()).unwrap_or_default();
        let mut next = be32(pg, start + local);
        while out.len() < total && next != 0 {
            let Some(op) = self.page_at(next) else { break };
            let take = (total - out.len()).min(u - 4);
            out.extend_from_slice(&op[4..4 + take]);
            next = be32(op, 0);
        }
        out
    }

    fn record(p: &[u8]) -> Vec<Value> {
        let (hlen, mut a) = varint(p, 0);
        let hlen = hlen as usize;
        let mut types = Vec::new();
        while a < hlen {
            let (t, n) = varint(p, a);
            types.push(t);
            a += n;
        }
        let mut pos = hlen;
        let mut out = Vec::with_capacity(types.len());
        for t in types {
            let size = match t {
                0 | 8 | 9 => 0,
                1 => 1,
                2 => 2,
                3 => 3,
                4 => 4,
                5 => 6,
                6 | 7 => 8,
                n if n >= 12 && n % 2 == 0 => ((n - 12) / 2) as usize,
                n if n >= 13 => ((n - 13) / 2) as usize,
                _ => 0,
            };
            let bytes = p.get(pos..pos + size).unwrap_or(&[]);
            pos += size;
            out.push(match t {
                0 => Value::Null,
                8 => Value::Int(0),
                9 => Value::Int(1),
                1..=6 => {
                    let mut v: i64 = if bytes.first().map(|b| b & 0x80 != 0).unwrap_or(false) { -1 } else { 0 };
                    for b in bytes {
                        v = (v << 8) | *b as i64;
                    }
                    Value::Int(v)
                }
                7 => Value::Real(f64::from_be_bytes(bytes.try_into().unwrap_or([0; 8]))),
                n if n >= 12 && n % 2 == 0 => Value::Blob(bytes.to_vec()),
                _ => Value::Text(String::from_utf8_lossy(bytes).to_string()),
            });
        }
        out
    }
}
