// Runtime for LNReader / IReader JavaScript plugins, run by QuickJS.
// It gives the plugins what they `require`: fetch, a small cheerio, htmlparser2, dayjs, storage and the helper libraries.
// The only things that come from the app are __http(method, url, headersJson, body) -> JSON text and __log(text).
(function (G) {
  'use strict';

  // ---------------------------------------------------------------- basics
  G.globalThis = G;
  G.window = G;
  G.self = G;
  const con = { log: (...a) => __log(a.map(String).join(' ')), warn: (...a) => __log(a.map(String).join(' ')), error: (...a) => __log(a.map(String).join(' ')), info: (...a) => __log(a.map(String).join(' ')), debug() {} };
  G.console = con;
  G.setTimeout = function (fn, ms, ...args) { Promise.resolve().then(() => fn(...args)); return 0; };
  G.clearTimeout = function () {};
  G.setInterval = function () { return 0; };
  G.clearInterval = function () {};
  G.queueMicrotask = G.queueMicrotask || ((f) => Promise.resolve().then(f));

  if (typeof G.Intl === 'undefined') {
    G.Intl = { DateTimeFormat: function () { return { resolvedOptions: () => ({ timeZone: 'UTC', locale: 'en-US' }), format: (d) => String(d) }; }, NumberFormat: function () { return { format: (n) => String(n) }; }, Collator: function () { return { compare: (a, b) => (a < b ? -1 : a > b ? 1 : 0) }; } };
  }
  const B64 = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/';
  G.btoa = G.btoa || function (s) {
    s = String(s);
    let o = '';
    for (let i = 0; i < s.length; i += 3) {
      const a = s.charCodeAt(i), b = s.charCodeAt(i + 1), c = s.charCodeAt(i + 2);
      const n = (a << 16) | ((b || 0) << 8) | (c || 0);
      o += B64[(n >> 18) & 63] + B64[(n >> 12) & 63] + (isNaN(b) ? '=' : B64[(n >> 6) & 63]) + (isNaN(c) ? '=' : B64[n & 63]);
    }
    return o;
  };
  G.atob = G.atob || function (s) {
    s = String(s).replace(/[^A-Za-z0-9+/]/g, '');
    let o = '';
    for (let i = 0; i < s.length; i += 4) {
      const n = (B64.indexOf(s[i]) << 18) | (B64.indexOf(s[i + 1]) << 12) | ((B64.indexOf(s[i + 2]) & 63) << 6) | (B64.indexOf(s[i + 3]) & 63);
      o += String.fromCharCode((n >> 16) & 255);
      if (s[i + 2] !== undefined) o += String.fromCharCode((n >> 8) & 255);
      if (s[i + 3] !== undefined) o += String.fromCharCode(n & 255);
    }
    return o;
  };

  function utf8Encode(str) {
    const out = [];
    for (const ch of String(str)) {
      let c = ch.codePointAt(0);
      if (c < 0x80) out.push(c);
      else if (c < 0x800) out.push(0xc0 | (c >> 6), 0x80 | (c & 63));
      else if (c < 0x10000) out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
      else out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63));
    }
    return new Uint8Array(out);
  }
  function utf8Decode(bytes) {
    let s = '';
    for (let i = 0; i < bytes.length;) {
      const b = bytes[i++];
      if (b < 0x80) s += String.fromCharCode(b);
      else if (b < 0xe0) s += String.fromCharCode(((b & 31) << 6) | (bytes[i++] & 63));
      else if (b < 0xf0) { s += String.fromCharCode(((b & 15) << 12) | ((bytes[i++] & 63) << 6) | (bytes[i++] & 63)); }
      else { const c = ((b & 7) << 18) | ((bytes[i++] & 63) << 12) | ((bytes[i++] & 63) << 6) | (bytes[i++] & 63); s += String.fromCodePoint(c); }
    }
    return s;
  }
  G.TextEncoder = G.TextEncoder || class { encode(s) { return utf8Encode(s); } };
  G.TextDecoder = G.TextDecoder || class { constructor(enc) { this.encoding = enc || 'utf-8'; } decode(b) { return b ? utf8Decode(b instanceof Uint8Array ? b : new Uint8Array(b)) : ''; } };

  // ---------------------------------------------------------------- URL and friends
  const enc = (s) => encodeURIComponent(s).replace(/%20/g, '+').replace(/[!'()~]/g, (c) => '%' + c.charCodeAt(0).toString(16).toUpperCase());
  const dec = (s) => { try { return decodeURIComponent(String(s).replace(/\+/g, ' ')); } catch (e) { return s; } };
  class URLSearchParams {
    constructor(init) {
      this._l = [];
      if (init instanceof URLSearchParams) this._l = init._l.map((p) => p.slice());
      else if (typeof init === 'string') {
        init.replace(/^\?/, '').split('&').forEach((p) => { if (p) { const i = p.indexOf('='); this._l.push(i < 0 ? [dec(p), ''] : [dec(p.slice(0, i)), dec(p.slice(i + 1))]); } });
      } else if (Array.isArray(init)) init.forEach((p) => this._l.push([String(p[0]), String(p[1])]));
      else if (init && typeof init === 'object') Object.keys(init).forEach((k) => this._l.push([k, String(init[k])]));
    }
    append(k, v) { this._l.push([String(k), String(v)]); }
    delete(k) { this._l = this._l.filter((p) => p[0] !== k); }
    get(k) { const p = this._l.find((p) => p[0] === k); return p ? p[1] : null; }
    getAll(k) { return this._l.filter((p) => p[0] === k).map((p) => p[1]); }
    has(k) { return this._l.some((p) => p[0] === k); }
    set(k, v) { const i = this._l.findIndex((p) => p[0] === k); if (i < 0) this._l.push([String(k), String(v)]); else { this._l[i][1] = String(v); this._l = this._l.filter((p, j) => p[0] !== k || j === i); } }
    sort() { this._l.sort((a, b) => (a[0] < b[0] ? -1 : a[0] > b[0] ? 1 : 0)); }
    forEach(f) { this._l.forEach((p) => f(p[1], p[0], this)); }
    keys() { return this._l.map((p) => p[0])[Symbol.iterator](); }
    values() { return this._l.map((p) => p[1])[Symbol.iterator](); }
    entries() { return this._l.map((p) => [p[0], p[1]])[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
    toString() { return this._l.map((p) => enc(p[0]) + '=' + enc(p[1])).join('&'); }
  }
  G.URLSearchParams = URLSearchParams;

  const DEFAULT_PORT = { 'http:': '80', 'https:': '443' };
  function resolvePath(base, rel) {
    const parts = base.split('/');
    parts.pop();
    rel.split('/').forEach((seg) => { if (seg === '..') { if (parts.length > 1) parts.pop(); } else if (seg !== '.') parts.push(seg); });
    return parts.join('/');
  }
  class URL {
    constructor(url, base) {
      url = String(url).trim();
      let m = /^([a-zA-Z][a-zA-Z0-9+.-]*:)(\/\/)?([^/?#]*)([^?#]*)(\?[^#]*)?(#.*)?$/.exec(url);
      if (!m || (!m[2] && !/^(mailto|data|javascript|tel):/i.test(url))) {
        if (base === undefined) throw new TypeError('Invalid URL: ' + url);
        const b = new URL(base);
        if (url.startsWith('//')) return new URL(b.protocol + url);
        let path, search = '', hash = '';
        let rest = url;
        const hi = rest.indexOf('#'); if (hi >= 0) { hash = rest.slice(hi); rest = rest.slice(0, hi); }
        const qi = rest.indexOf('?'); if (qi >= 0) { search = rest.slice(qi); rest = rest.slice(0, qi); }
        if (rest === '') { path = b.pathname; if (qi < 0) search = b.search; } else if (rest.startsWith('/')) path = resolvePath('/x', rest.slice(1) ? rest : '/');
        else path = resolvePath(b.pathname, rest);
        if (rest.startsWith('/')) path = resolvePath('', rest.slice(1));
        if (!path.startsWith('/')) path = '/' + path;
        return new URL(b.protocol + '//' + b.host + path + search + hash);
      }
      this.protocol = m[1].toLowerCase();
      const auth = m[3];
      const at = auth.lastIndexOf('@');
      const hostport = at >= 0 ? auth.slice(at + 1) : auth;
      const ui = at >= 0 ? auth.slice(0, at) : '';
      this.username = ui.split(':')[0] || '';
      this.password = ui.split(':')[1] || '';
      const pm = /^(.*?)(?::(\d+))?$/.exec(hostport);
      this.hostname = (pm[1] || '').toLowerCase();
      this.port = pm[2] && pm[2] !== DEFAULT_PORT[this.protocol] ? pm[2] : '';
      this.host = this.hostname + (this.port ? ':' + this.port : '');
      this.pathname = m[4] || (m[2] ? '/' : '');
      if (m[2] && this.pathname) this.pathname = resolvePath('', this.pathname.replace(/^\//, '')).replace(/^(?!\/)/, '/');
      this._search = new URLSearchParams(m[5] || '');
      this._rawSearch = m[5] || '';
      this.hash = m[6] || '';
    }
    get search() { const s = this._search.toString(); return s ? '?' + s : (this._rawSearch === '?' ? '' : ''); }
    set search(v) { this._search = new URLSearchParams(v); }
    get searchParams() { return this._search; }
    get origin() { return this.protocol + '//' + this.host; }
    get href() { return this.protocol + '//' + (this.username ? this.username + (this.password ? ':' + this.password : '') + '@' : '') + this.host + this.pathname + this.search + this.hash; }
    toString() { return this.href; }
    toJSON() { return this.href; }
  }
  G.URL = URL;

  class Headers {
    constructor(init) {
      this._m = {};
      if (init instanceof Headers) init.forEach((v, k) => this.set(k, v));
      else if (Array.isArray(init)) init.forEach((p) => this.append(p[0], p[1]));
      else if (init && typeof init === 'object') Object.keys(init).forEach((k) => this.set(k, init[k]));
    }
    append(k, v) { k = String(k).toLowerCase(); this._m[k] = this._m[k] !== undefined ? this._m[k] + ', ' + v : String(v); }
    set(k, v) { this._m[String(k).toLowerCase()] = String(v); }
    get(k) { const v = this._m[String(k).toLowerCase()]; return v === undefined ? null : v; }
    has(k) { return this._m[String(k).toLowerCase()] !== undefined; }
    delete(k) { delete this._m[String(k).toLowerCase()]; }
    forEach(f) { Object.keys(this._m).forEach((k) => f(this._m[k], k, this)); }
    entries() { return Object.keys(this._m).map((k) => [k, this._m[k]])[Symbol.iterator](); }
    keys() { return Object.keys(this._m)[Symbol.iterator](); }
    values() { return Object.keys(this._m).map((k) => this._m[k])[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
  }
  G.Headers = Headers;

  class FormData {
    constructor() { this._l = []; }
    append(k, v) { this._l.push([String(k), v]); }
    set(k, v) { this.delete(k); this.append(k, v); }
    delete(k) { this._l = this._l.filter((p) => p[0] !== k); }
    get(k) { const p = this._l.find((p) => p[0] === k); return p ? p[1] : null; }
    getAll(k) { return this._l.filter((p) => p[0] === k).map((p) => p[1]); }
    has(k) { return this._l.some((p) => p[0] === k); }
    forEach(f) { this._l.forEach((p) => f(p[1], p[0], this)); }
    entries() { return this._l.map((p) => [p[0], p[1]])[Symbol.iterator](); }
    [Symbol.iterator]() { return this.entries(); }
  }
  G.FormData = FormData;

  class AbortSignal { constructor() { this.aborted = false; this._f = []; } addEventListener(t, f) { this._f.push(f); } removeEventListener() {} }
  G.AbortSignal = AbortSignal;
  G.AbortController = class { constructor() { this.signal = new AbortSignal(); } abort() { this.signal.aborted = true; this.signal._f.forEach((f) => f()); } };

  // ---------------------------------------------------------------- HTML: parser
  const VOID = new Set('area base br col embed hr img input link meta param source track wbr command keygen frame basefont isindex'.split(' '));
  const RAW = new Set(['script', 'style', 'textarea', 'title']);
  const ENT = { amp: '&', lt: '<', gt: '>', quot: '"', apos: "'", nbsp: '\u00a0', hellip: '…', mdash: '—', ndash: '–', lsquo: '‘', rsquo: '’', ldquo: '“', rdquo: '”', laquo: '«', raquo: '»', copy: '©', reg: '®', trade: '™', times: '×', middot: '·', bull: '•', deg: '°', euro: '€', pound: '£', yen: '¥', cent: '¢', sect: '§', para: '¶', iexcl: '¡', iquest: '¿', shy: '\u00ad', zwnj: '\u200c', zwj: '\u200d', ensp: '\u2002', emsp: '\u2003', thinsp: '\u2009', agrave: 'à', aacute: 'á', acirc: 'â', atilde: 'ã', auml: 'ä', aring: 'å', ccedil: 'ç', egrave: 'è', eacute: 'é', ecirc: 'ê', euml: 'ë', igrave: 'ì', iacute: 'í', icirc: 'î', iuml: 'ï', ntilde: 'ñ', ograve: 'ò', oacute: 'ó', ocirc: 'ô', otilde: 'õ', ouml: 'ö', ugrave: 'ù', uacute: 'ú', ucirc: 'û', uuml: 'ü', yacute: 'ý', szlig: 'ß', Agrave: 'À', Aacute: 'Á', Eacute: 'É', Ntilde: 'Ñ', Oacute: 'Ó', Uuml: 'Ü', larr: '←', rarr: '→', uarr: '↑', darr: '↓', hearts: '♥', frac12: '½', frac14: '¼', frac34: '¾', plusmn: '±', minus: '−', infin: '∞', ne: '≠', le: '≤', ge: '≥', prime: '′', Prime: '″', dagger: '†', permil: '‰', sbquo: '‚', bdquo: '„', lsaquo: '‹', rsaquo: '›' };
  function decodeEntities(s) {
    if (s.indexOf('&') < 0) return s;
    return s.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[A-Za-z][A-Za-z0-9]*);?/g, (m, e) => {
      if (e[0] === '#') {
        const n = e[1] === 'x' || e[1] === 'X' ? parseInt(e.slice(2), 16) : parseInt(e.slice(1), 10);
        try { return String.fromCodePoint(n); } catch (x) { return m; }
      }
      return ENT[e] !== undefined ? ENT[e] : m;
    });
  }
  const CLOSES = { p: ['p'], li: ['li'], dt: ['dt', 'dd'], dd: ['dt', 'dd'], tr: ['tr', 'td', 'th'], td: ['td', 'th'], th: ['td', 'th'], option: ['option'], thead: ['tbody', 'tfoot'], tbody: ['thead', 'tbody', 'tfoot'] };
  const P_CLOSERS = new Set('address article aside blockquote div dl fieldset footer form h1 h2 h3 h4 h5 h6 header hr main nav ol p pre section table ul'.split(' '));

  // A streaming tokenizer used by both the DOM builder and the htmlparser2 Parser.
  function tokenize(html, h) {
    const n = html.length;
    let i = 0;
    while (i < n) {
      const lt = html.indexOf('<', i);
      if (lt < 0) { h.text(html.slice(i)); break; }
      if (lt > i) h.text(html.slice(i, lt));
      i = lt;
      if (html.startsWith('<!--', i)) {
        const e = html.indexOf('-->', i + 4);
        const end = e < 0 ? n : e;
        h.comment(html.slice(i + 4, end));
        i = e < 0 ? n : e + 3;
        continue;
      }
      if (html.startsWith('<![CDATA[', i)) {
        const e = html.indexOf(']]>', i);
        h.text(html.slice(i + 9, e < 0 ? n : e));
        i = e < 0 ? n : e + 3;
        continue;
      }
      if (html[i + 1] === '!' || html[i + 1] === '?') {
        const e = html.indexOf('>', i);
        i = e < 0 ? n : e + 1;
        continue;
      }
      if (html[i + 1] === '/') {
        const e = html.indexOf('>', i);
        const name = html.slice(i + 2, e < 0 ? n : e).trim().split(/\s/)[0].toLowerCase();
        if (name) h.close(name);
        i = e < 0 ? n : e + 1;
        continue;
      }
      if (!/[a-zA-Z]/.test(html[i + 1] || '')) { h.text('<'); i++; continue; }
      // opening tag
      let j = i + 1;
      while (j < n && !/[\s/>]/.test(html[j])) j++;
      const name = html.slice(i + 1, j).toLowerCase();
      const attribs = {};
      let selfClose = false;
      for (;;) {
        while (j < n && /\s/.test(html[j])) j++;
        if (j >= n) break;
        if (html[j] === '>') { j++; break; }
        if (html[j] === '/') { if (html[j + 1] === '>') { selfClose = true; j += 2; break; } j++; continue; }
        let k = j;
        while (k < n && !/[\s=/>]/.test(html[k])) k++;
        const an = html.slice(j, k).toLowerCase();
        j = k;
        while (j < n && /\s/.test(html[j])) j++;
        let val = '';
        if (html[j] === '=') {
          j++;
          while (j < n && /\s/.test(html[j])) j++;
          if (html[j] === '"' || html[j] === "'") {
            const q = html[j];
            const e = html.indexOf(q, j + 1);
            val = html.slice(j + 1, e < 0 ? n : e);
            j = e < 0 ? n : e + 1;
          } else {
            k = j;
            while (k < n && !/[\s>]/.test(html[k])) k++;
            val = html.slice(j, k);
            j = k;
          }
        }
        if (an && !(an in attribs)) attribs[an] = decodeEntities(val);
        if (!an) j++;
      }
      h.open(name, attribs, selfClose);
      i = j;
      if (RAW.has(name) && !selfClose) {
        const re = new RegExp('</' + name + '\\s*>', 'i');
        const m = re.exec(html.slice(i));
        const end = m ? i + m.index : n;
        if (end > i) h.raw(html.slice(i, end), name);
        i = m ? end : n;
      }
    }
  }

  // ---------------------------------------------------------------- HTML: DOM
  class Node {
    constructor(type) { this.type = type; this.parent = null; this.children = []; }
    get nodeType() { return this.type === 'tag' ? 1 : this.type === 'text' ? 3 : this.type === 'comment' ? 8 : 9; }
    get tagName() { return this.name ? this.name.toUpperCase() : undefined; }
    get firstChild() { return this.children[0] || null; }
    get childNodes() { return this.children; }
    get nextSibling() { return sibling(this, 1); }
    get previousSibling() { return sibling(this, -1); }
    get parentNode() { return this.parent; }
    get data() { return this.type === 'text' || this.type === 'comment' ? this.value : undefined; }
    get nodeValue() { return this.data; }
  }
  function sibling(n, d) {
    if (!n.parent) return null;
    const c = n.parent.children;
    return c[c.indexOf(n) + d] || null;
  }
  function mkTag(name, attribs) { const n = new Node('tag'); n.name = name; n.attribs = attribs || {}; return n; }
  function mkText(v, raw) { const n = new Node('text'); n.value = v; n.raw = !!raw; return n; }
  function link(parent, child) { child.parent = parent; parent.children.push(child); }

  function parseHTML(html) {
    const root = new Node('root');
    const stack = [root];
    const top = () => stack[stack.length - 1];
    tokenize(String(html), {
      text(t) { if (t) link(top(), mkText(decodeEntities(t))); },
      raw(t, name) { if (name === 'title' || name === 'textarea') link(top(), mkText(decodeEntities(t))); else link(top(), mkText(t, true)); },
      comment(t) { const c = new Node('comment'); c.value = t; link(top(), c); },
      open(name, attribs, selfClose) {
        const closes = CLOSES[name];
        if (closes) {
          for (let k = stack.length - 1; k > 0; k--) {
            const nm = stack[k].name;
            if (closes.includes(nm)) { stack.length = k; break; }
            if (['table', 'ul', 'ol', 'div', 'body', 'html', 'select'].includes(nm)) break;
          }
        }
        if (P_CLOSERS.has(name)) {
          for (let k = stack.length - 1; k > 0; k--) {
            const nm = stack[k].name;
            if (nm === 'p') { stack.length = k; break; }
            if (!['span', 'a', 'b', 'i', 'em', 'strong', 'u', 'small', 'font'].includes(nm)) break;
          }
        }
        const t = mkTag(name, attribs);
        link(top(), t);
        if (!selfClose && !VOID.has(name)) stack.push(t);
      },
      close(name) {
        for (let k = stack.length - 1; k > 0; k--) if (stack[k].name === name) { stack.length = k; return; }
      },
    });
    return root;
  }

  const escText = (s) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  const escAttr = (s) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
  function outer(n) {
    if (n.type === 'text') return n.raw ? n.value : escText(n.value);
    if (n.type === 'comment') return '<!--' + n.value + '-->';
    if (n.type === 'root') return inner(n);
    let s = '<' + n.name;
    for (const k of Object.keys(n.attribs)) s += ' ' + k + '="' + escAttr(n.attribs[k]) + '"';
    if (VOID.has(n.name)) return s + '>';
    return s + '>' + inner(n) + '</' + n.name + '>';
  }
  function inner(n) { return n.children.map(outer).join(''); }
  function textOf(n) {
    if (n.type === 'text') return n.value;
    if (n.type === 'comment') return '';
    return n.children.map(textOf).join('');
  }

  // ---------------------------------------------------------------- selectors
  function parseSelector(sel) {
    // returns a list of alternatives; each is an array of {comb, tag, id, classes, attrs, pseudos} from left to right
    const src = String(sel).trim();
    let i = 0;
    const alts = [];
    let cur = [];
    let comb = ' ';
    const isId = (c) => /[A-Za-z0-9_\-\u00a0-\uffff\\]/.test(c);
    const ident = () => {
      let s = '';
      while (i < src.length && isId(src[i])) { if (src[i] === '\\') { i++; } s += src[i++]; }
      return s;
    };
    const skipWs = () => { while (i < src.length && /\s/.test(src[i])) i++; };
    const compound = () => {
      const c = { comb, tag: null, id: null, classes: [], attrs: [], pseudos: [] };
      let any = false;
      if (src[i] === '*') { i++; any = true; } else if (i < src.length && isId(src[i]) ) { c.tag = ident().toLowerCase(); any = true; }
      for (;;) {
        const ch = src[i];
        if (ch === '#') { i++; c.id = ident(); any = true; }
        else if (ch === '.') { i++; c.classes.push(ident()); any = true; }
        else if (ch === '[') {
          i++;
          skipWs();
          const name = ident().toLowerCase();
          skipWs();
          let op = null, val = '', ci = false;
          if (src[i] !== ']') {
            op = '';
            while (src[i] !== '=' && src[i] !== ']') op += src[i++];
            if (src[i] === '=') { op += '='; i++; }
            skipWs();
            if (src[i] === '"' || src[i] === "'") { const q = src[i++]; while (i < src.length && src[i] !== q) val += src[i++]; i++; }
            else { while (i < src.length && src[i] !== ']' && !/\s/.test(src[i])) val += src[i++]; }
            skipWs();
            if (/[iI]/.test(src[i] || '') && src[i + 1] === ']') { ci = true; i++; }
          }
          if (src[i] === ']') i++;
          c.attrs.push({ name, op, val, ci });
          any = true;
        } else if (ch === ':') {
          i++;
          if (src[i] === ':') i++;
          const name = ident().toLowerCase();
          let arg = null;
          if (src[i] === '(') {
            let depth = 1; i++;
            let s = '';
            while (i < src.length && depth > 0) { if (src[i] === '(') depth++; else if (src[i] === ')') { depth--; if (depth === 0) break; } s += src[i++]; }
            i++;
            arg = s.trim();
          }
          c.pseudos.push({ name, arg });
          any = true;
        } else break;
      }
      if (!any) throw new Error('Bad selector: ' + src);
      return c;
    };
    skipWs();
    while (i < src.length) {
      cur.push(compound());
      skipWs();
      if (i >= src.length) break;
      const ch = src[i];
      if (ch === ',') { alts.push(cur); cur = []; i++; skipWs(); comb = ' '; }
      else if (ch === '>' || ch === '+' || ch === '~') { comb = ch; i++; skipWs(); }
      else comb = ' ';
    }
    if (cur.length) alts.push(cur);
    return alts;
  }
  const elChildren = (n) => (n.children || []).filter((c) => c.type === 'tag');
  function nth(expr, idx) {
    expr = String(expr).replace(/\s+/g, '').toLowerCase();
    if (expr === 'odd') expr = '2n+1'; else if (expr === 'even') expr = '2n';
    const m = /^([+-]?\d*)n([+-]\d+)?$/.exec(expr);
    if (!m) return parseInt(expr, 10) === idx;
    const a = m[1] === '' || m[1] === '+' ? 1 : m[1] === '-' ? -1 : parseInt(m[1], 10);
    const b = m[2] ? parseInt(m[2], 10) : 0;
    if (a === 0) return idx === b;
    const k = (idx - b) / a;
    return Number.isInteger(k) && k >= 0;
  }
  function matchSimple(n, c, scope) {
    if (n.type !== 'tag') return false;
    if (c.tag && n.name !== c.tag) return false;
    if (c.id !== null && n.attribs.id !== c.id) return false;
    if (c.classes.length) {
      const cl = (n.attribs.class || '').split(/\s+/);
      for (const k of c.classes) if (!cl.includes(k)) return false;
    }
    for (const a of c.attrs) {
      const has = Object.prototype.hasOwnProperty.call(n.attribs, a.name);
      if (a.op === null) { if (!has) return false; continue; }
      let v = has ? n.attribs[a.name] : null;
      let w = a.val;
      if (a.op === '!=') { if (v === w) return false; continue; }
      if (v === null) return false;
      if (a.ci) { v = v.toLowerCase(); w = w.toLowerCase(); }
      switch (a.op) {
        case '=': if (v !== w) return false; break;
        case '~=': if (!v.split(/\s+/).includes(w)) return false; break;
        case '|=': if (v !== w && !v.startsWith(w + '-')) return false; break;
        case '^=': if (!w || !v.startsWith(w)) return false; break;
        case '$=': if (!w || !v.endsWith(w)) return false; break;
        case '*=': if (!w || v.indexOf(w) < 0) return false; break;
        default: return false;
      }
    }
    for (const p of c.pseudos) if (!matchPseudo(n, p, scope)) return false;
    return true;
  }
  function matchPseudo(n, p, scope) {
    const sibs = n.parent ? elChildren(n.parent) : [n];
    const idx = sibs.indexOf(n) + 1;
    const same = sibs.filter((s) => s.name === n.name);
    switch (p.name) {
      case 'first-child': return idx === 1;
      case 'last-child': return idx === sibs.length;
      case 'only-child': return sibs.length === 1;
      case 'nth-child': return nth(p.arg, idx);
      case 'nth-last-child': return nth(p.arg, sibs.length - idx + 1);
      case 'first-of-type': return same[0] === n;
      case 'last-of-type': return same[same.length - 1] === n;
      case 'only-of-type': return same.length === 1;
      case 'nth-of-type': return nth(p.arg, same.indexOf(n) + 1);
      case 'nth-last-of-type': return nth(p.arg, same.length - same.indexOf(n));
      case 'empty': return n.children.every((c) => c.type === 'comment');
      case 'root': return !!n.parent && n.parent.type === 'root';
      case 'not': return !matches(n, p.arg, scope);
      case 'is': case 'where': case 'matches': return matches(n, p.arg, scope);
      case 'has': return selectAll(p.arg, [n]).length > 0;
      case 'contains': return textOf(n).indexOf(String(p.arg).replace(/^["']|["']$/g, '')) >= 0;
      case 'icontains': return textOf(n).toLowerCase().indexOf(String(p.arg).replace(/^["']|["']$/g, '').toLowerCase()) >= 0;
      case 'checked': return 'checked' in n.attribs;
      case 'disabled': return 'disabled' in n.attribs;
      case 'enabled': return !('disabled' in n.attribs);
      case 'link': case 'any-link': return n.name === 'a' && 'href' in n.attribs;
      case 'header': return /^h[1-6]$/.test(n.name);
      case 'input': return ['input', 'select', 'textarea', 'button'].includes(n.name);
      case 'button': return n.name === 'button' || (n.name === 'input' && n.attribs.type === 'button');
      case 'selected': return 'selected' in n.attribs;
      default: return false; // :first, :last, :eq... are handled on the result set
    }
  }
  const SET_PSEUDO = new Set(['first', 'last', 'eq', 'gt', 'lt', 'even', 'odd']);
  function matchChain(n, chain, k, scope) {
    const c = chain[k];
    if (!matchSimple(n, c, scope)) return false;
    if (k === 0) return true;
    switch (c.comb) {
      case '>': return !!n.parent && n.parent.type === 'tag' && matchChain(n.parent, chain, k - 1, scope);
      case '+': { const s = prevEl(n); return !!s && matchChain(s, chain, k - 1, scope); }
      case '~': { let s = prevEl(n); while (s) { if (matchChain(s, chain, k - 1, scope)) return true; s = prevEl(s); } return false; }
      default: { let p = n.parent; while (p && p.type === 'tag') { if (matchChain(p, chain, k - 1, scope)) return true; p = p.parent; } return false; }
    }
  }
  function prevEl(n) {
    if (!n.parent) return null;
    const c = n.parent.children;
    for (let i = c.indexOf(n) - 1; i >= 0; i--) if (c[i].type === 'tag') return c[i];
    return null;
  }
  const cache = new Map();
  function compile(sel) {
    let r = cache.get(sel);
    if (!r) {
      // set pseudos (:first, :last, :eq(n)...) at the end of a top-level selector
      const post = [];
      let s = sel;
      for (;;) {
        const m = /:(first|last|eq|gt|lt|even|odd)(?:\(([^)]*)\))?\s*$/.exec(s);
        if (!m) break;
        post.unshift({ name: m[1], arg: m[2] });
        s = s.slice(0, m.index) || '*';
      }
      r = { alts: parseSelector(s), post };
      if (cache.size > 500) cache.clear();
      cache.set(sel, r);
    }
    return r;
  }
  function matches(n, sel, scope) {
    return compile(sel).alts.some((chain) => matchChain(n, chain, chain.length - 1, scope));
  }
  function selectAll(sel, roots) {
    const { alts, post } = compile(sel);
    const out = [];
    const seen = new Set();
    const walk = (n) => {
      if (n.type === 'tag' && !seen.has(n) && alts.some((chain) => matchChain(n, chain, chain.length - 1))) { seen.add(n); out.push(n); }
      for (const c of n.children) walk(c);
    };
    for (const r of roots) for (const c of r.children) walk(c);
    let res = out;
    for (const p of post) {
      const a = p.arg === undefined ? undefined : parseInt(p.arg, 10);
      if (p.name === 'first') res = res.slice(0, 1);
      else if (p.name === 'last') res = res.slice(-1);
      else if (p.name === 'eq') res = res.slice(a < 0 ? res.length + a : a, (a < 0 ? res.length + a : a) + 1);
      else if (p.name === 'gt') res = res.slice(a + 1);
      else if (p.name === 'lt') res = res.slice(0, a);
      else if (p.name === 'even') res = res.filter((_, i) => i % 2 === 0);
      else if (p.name === 'odd') res = res.filter((_, i) => i % 2 === 1);
    }
    return res;
  }

  // ---------------------------------------------------------------- cheerio
  function uniq(list) { const s = new Set(); return list.filter((n) => (s.has(n) ? false : (s.add(n), true))); }
  class Cheerio {
    constructor(nodes, prev, root) {
      const list = nodes || [];
      this.length = list.length;
      for (let i = 0; i < list.length; i++) this[i] = list[i];
      Object.defineProperty(this, 'prevObject', { value: prev, enumerable: false, writable: true });
      Object.defineProperty(this, '_root', { value: root || (prev && prev._root), enumerable: false, writable: true });
    }
    _n() { return Array.prototype.slice.call(this); }
    _wrap(nodes) { return new Cheerio(nodes, this, this._root); }
    [Symbol.iterator]() { return this._n()[Symbol.iterator](); }
    toArray() { return this._n(); }
    get(i) { return i === undefined ? this._n() : this[i < 0 ? this.length + i : i]; }
    eq(i) { i = i < 0 ? this.length + i : i; return this._wrap(this[i] ? [this[i]] : []); }
    first() { return this.eq(0); }
    last() { return this.eq(-1); }
    slice(a, b) { return this._wrap(this._n().slice(a, b)); }
    splice(a, b, ...r) { return this._n().splice(a, b, ...r); }
    addBack(sel) { const p = this.prevObject ? (sel ? this.prevObject.filter(sel) : this.prevObject)._n() : []; return this._wrap(uniq(p.concat(this._n()))); }
    end() { return this.prevObject || new Cheerio([], null, this._root); }
    add(sel, ctx) { const more = this._root.$(sel, ctx).toArray(); return this._wrap(uniq(this._n().concat(more))); }
    each(fn) { for (let i = 0; i < this.length; i++) if (fn.call(this[i], i, this[i]) === false) break; return this; }
    map(fn) {
      const out = [];
      for (let i = 0; i < this.length; i++) {
        const r = fn.call(this[i], i, this[i]);
        if (r === null || r === undefined) continue;
        if (Array.isArray(r)) out.push(...r); else out.push(r);
      }
      return this._wrap(out);
    }
    filter(f) {
      const list = this._n();
      if (typeof f === 'function') return this._wrap(list.filter((n, i) => f.call(n, i, n)));
      if (typeof f === 'string') return this._wrap(list.filter((n) => matches(n, f)));
      const set = new Set(f instanceof Cheerio ? f._n() : [].concat(f));
      return this._wrap(list.filter((n) => set.has(n)));
    }
    not(f) {
      const keep = this.filter(f);
      const set = new Set(keep._n());
      return this._wrap(this._n().filter((n) => !set.has(n)));
    }
    has(f) {
      return this._wrap(this._n().filter((n) => (typeof f === 'string' ? selectAll(f, [n]).length > 0 : (f instanceof Cheerio ? f._n() : [f]).some((x) => { for (let p = x.parent; p; p = p.parent) if (p === n) return true; return false; }))));
    }
    is(f) {
      if (typeof f === 'function') return this._n().some((n, i) => f.call(n, i, n));
      if (typeof f === 'string') return this._n().some((n) => matches(n, f));
      const set = new Set(f instanceof Cheerio ? f._n() : [].concat(f));
      return this._n().some((n) => set.has(n));
    }
    find(sel) {
      if (typeof sel !== 'string') { const set = new Set(sel instanceof Cheerio ? sel._n() : [sel]); return this._wrap(selectAllNodes(this._n()).filter((n) => set.has(n))); }
      return this._wrap(uniq(selectAll(sel, this._n())));
    }
    children(sel) {
      let out = [];
      this._n().forEach((n) => { out = out.concat(elChildren(n)); });
      out = uniq(out);
      return this._wrap(sel ? out.filter((n) => matches(n, sel)) : out);
    }
    contents() { let out = []; this._n().forEach((n) => { out = out.concat(n.children || []); }); return this._wrap(out); }
    parent(sel) {
      let out = uniq(this._n().map((n) => n.parent).filter((p) => p && p.type === 'tag'));
      if (sel) out = out.filter((n) => matches(n, sel));
      return this._wrap(out);
    }
    parents(sel) {
      let out = [];
      this._n().forEach((n) => { for (let p = n.parent; p && p.type === 'tag'; p = p.parent) out.push(p); });
      out = uniq(out);
      return this._wrap(sel ? out.filter((n) => matches(n, sel)) : out);
    }
    closest(sel) {
      const out = [];
      this._n().forEach((n) => { for (let p = n; p && p.type === 'tag'; p = p.parent) if (!sel || matches(p, sel)) { out.push(p); break; } });
      return this._wrap(uniq(out));
    }
    next(sel) {
      let out = this._n().map((n) => nextEl(n)).filter(Boolean);
      if (sel) out = out.filter((n) => matches(n, sel));
      return this._wrap(uniq(out));
    }
    nextAll(sel) {
      let out = [];
      this._n().forEach((n) => { for (let s = nextEl(n); s; s = nextEl(s)) out.push(s); });
      out = uniq(out);
      return this._wrap(sel ? out.filter((n) => matches(n, sel)) : out);
    }
    nextUntil(sel) {
      const out = [];
      this._n().forEach((n) => { for (let s = nextEl(n); s; s = nextEl(s)) { if (sel && (typeof sel === 'string' ? matches(s, sel) : false)) break; out.push(s); } });
      return this._wrap(uniq(out));
    }
    prev(sel) {
      let out = this._n().map((n) => prevEl(n)).filter(Boolean);
      if (sel) out = out.filter((n) => matches(n, sel));
      return this._wrap(uniq(out));
    }
    prevAll(sel) {
      let out = [];
      this._n().forEach((n) => { for (let s = prevEl(n); s; s = prevEl(s)) out.push(s); });
      out = uniq(out);
      return this._wrap(sel ? out.filter((n) => matches(n, sel)) : out);
    }
    siblings(sel) {
      let out = [];
      this._n().forEach((n) => { if (n.parent) out = out.concat(elChildren(n.parent).filter((s) => s !== n)); });
      out = uniq(out);
      return this._wrap(sel ? out.filter((n) => matches(n, sel)) : out);
    }
    index(sel) {
      const n = this[0];
      if (!n || !n.parent) return -1;
      return elChildren(n.parent).indexOf(n);
    }
    hasClass(c) { return this._n().some((n) => n.type === 'tag' && (n.attribs.class || '').split(/\s+/).includes(c)); }
    addClass(c) {
      const add = String(c).split(/\s+/).filter(Boolean);
      this._n().forEach((n) => { if (n.type === 'tag') { const cur = (n.attribs.class || '').split(/\s+/).filter(Boolean); add.forEach((x) => { if (!cur.includes(x)) cur.push(x); }); n.attribs.class = cur.join(' '); } });
      return this;
    }
    removeClass(c) {
      const del = c === undefined ? null : String(c).split(/\s+/);
      this._n().forEach((n) => { if (n.type === 'tag') { if (!del) delete n.attribs.class; else n.attribs.class = (n.attribs.class || '').split(/\s+/).filter((x) => x && !del.includes(x)).join(' '); } });
      return this;
    }
    attr(name, val) {
      if (name && typeof name === 'object') { Object.keys(name).forEach((k) => this.attr(k, name[k])); return this; }
      if (val === undefined) {
        const n = this[0];
        if (!n || n.type !== 'tag') return undefined;
        return Object.prototype.hasOwnProperty.call(n.attribs, name) ? n.attribs[name] : undefined;
      }
      this._n().forEach((n) => { if (n.type === 'tag') { if (val === null) delete n.attribs[name]; else n.attribs[name] = String(val); } });
      return this;
    }
    removeAttr(name) { this._n().forEach((n) => { if (n.type === 'tag') String(name).split(/\s+/).forEach((k) => delete n.attribs[k]); }); return this; }
    prop(name, val) {
      const n = this[0];
      if (val !== undefined) { return this.attr(name, val); }
      if (!n) return undefined;
      if (name === 'tagName' || name === 'nodeName') return n.name ? n.name.toUpperCase() : undefined;
      if (name === 'outerHTML') return outer(n);
      if (name === 'innerHTML') return inner(n);
      if (name === 'textContent') return textOf(n);
      if (name === 'className') return n.attribs.class || '';
      if (name === 'children') return elChildren(n);
      if (name === 'childNodes') return n.children;
      if (name === 'href' || name === 'src') return n.attribs[name];
      return n.attribs ? n.attribs[name] : undefined;
    }
    data(k) { const n = this[0]; if (!n || !n.attribs) return undefined; if (k === undefined) { const o = {}; Object.keys(n.attribs).forEach((a) => { if (a.startsWith('data-')) o[a.slice(5)] = n.attribs[a]; }); return o; } return n.attribs['data-' + k]; }
    val(v) { if (v === undefined) { const n = this[0]; return n && n.attribs ? n.attribs.value : undefined; } return this.attr('value', v); }
    css() { return undefined; }
    text(v) {
      if (v !== undefined) { this._n().forEach((n) => { n.children = []; link(n, mkText(String(v))); }); return this; }
      return this._n().map(textOf).join('');
    }
    html(v) {
      if (v === undefined) { const n = this[0]; return n && n.type !== 'text' ? inner(n) : null; }
      this._n().forEach((n) => { n.children = []; toNodes(v).forEach((c) => { detach(c); link(n, c); }); });
      return this;
    }
    outerHTML() { return this[0] ? outer(this[0]) : null; }
    xml() { return this._n().map(outer).join(''); }
    clone() { return this._wrap(this._n().map((n) => cloneNode(n))); }
    remove(sel) { (sel ? this.filter(sel) : this)._n().forEach(detach); return this; }
    empty() { this._n().forEach((n) => { n.children = []; }); return this; }
    append(...a) { const nodes = flat(a); this._n().forEach((n, k) => { nodes.forEach((c) => { const x = k === 0 ? c : cloneNode(c); detach(x); link(n, x); }); }); return this; }
    prepend(...a) { const nodes = flat(a); this._n().forEach((n, k) => { nodes.slice().reverse().forEach((c) => { const x = k === 0 ? c : cloneNode(c); detach(x); x.parent = n; n.children.unshift(x); }); }); return this; }
    after(...a) { const nodes = flat(a); this._n().forEach((n, k) => { if (!n.parent) return; const p = n.parent; nodes.forEach((c, j) => { const x = k === 0 ? c : cloneNode(c); detach(x); x.parent = p; p.children.splice(p.children.indexOf(n) + 1 + j, 0, x); }); }); return this; }
    before(...a) { const nodes = flat(a); this._n().forEach((n, k) => { if (!n.parent) return; const p = n.parent; nodes.forEach((c, j) => { const x = k === 0 ? c : cloneNode(c); detach(x); x.parent = p; p.children.splice(p.children.indexOf(n) + j, 0, x); }); }); return this; }
    replaceWith(...a) { const nodes = flat(a); this._n().forEach((n, k) => { if (!n.parent) return; const p = n.parent; const at = p.children.indexOf(n); nodes.forEach((c, j) => { const x = k === 0 ? c : cloneNode(c); detach(x); x.parent = p; p.children.splice(at + j, 0, x); }); detach(n); }); return this; }
    wrap(w) { this._n().forEach((n) => { const nodes = toNodes(w); const wrapper = nodes.find((x) => x.type === 'tag'); if (!wrapper || !n.parent) return; const p = n.parent; const at = p.children.indexOf(n); p.children[at] = wrapper; wrapper.parent = p; let leaf = wrapper; while (elChildren(leaf)[0]) leaf = elChildren(leaf)[0]; n.parent = leaf; leaf.children.push(n); }); return this; }
    serialize() { return this.xml(); }
  }
  function nextEl(n) { if (!n.parent) return null; const c = n.parent.children; for (let i = c.indexOf(n) + 1; i < c.length; i++) if (c[i].type === 'tag') return c[i]; return null; }
  function selectAllNodes(roots) { const out = []; const walk = (n) => { if (n.type === 'tag') out.push(n); n.children.forEach(walk); }; roots.forEach((r) => r.children.forEach(walk)); return out; }
  function detach(n) { if (n.parent) { const c = n.parent.children; const i = c.indexOf(n); if (i >= 0) c.splice(i, 1); n.parent = null; } }
  function cloneNode(n) {
    const c = new Node(n.type);
    if (n.name !== undefined) c.name = n.name;
    if (n.attribs) c.attribs = Object.assign({}, n.attribs);
    if (n.value !== undefined) { c.value = n.value; c.raw = n.raw; }
    n.children.forEach((k) => link(c, cloneNode(k)));
    return c;
  }
  function toNodes(x) {
    if (x === null || x === undefined) return [];
    if (typeof x === 'string') return parseHTML(x).children.slice().map((c) => { c.parent = null; return c; });
    if (x instanceof Cheerio) return x._n();
    if (Array.isArray(x)) return flat(x);
    if (x instanceof Node) return [x];
    return [mkText(String(x))];
  }
  function flat(args) { let out = []; args.forEach((a) => { out = out.concat(toNodes(a)); }); return out; }

  function load(html, options, isDocument) {
    const root = typeof html === 'string' || html === undefined ? parseHTML(html || '') : html instanceof Node ? html : parseHTML(String(html));
    const api = {};
    const $ = function (sel, ctx) {
      if (sel === undefined || sel === null) return new Cheerio([], null, api);
      if (sel instanceof Cheerio) return sel;
      if (sel instanceof Node) return new Cheerio([sel], null, api);
      if (Array.isArray(sel)) return new Cheerio(sel, null, api);
      if (typeof sel === 'string') {
        if (/^\s*</.test(sel)) { return new Cheerio(parseHTML(sel).children, null, api); }
        let roots = [root];
        if (ctx !== undefined) roots = ctx instanceof Cheerio ? ctx._n() : typeof ctx === 'string' ? $(ctx)._n() : [].concat(ctx);
        return new Cheerio(uniq(selectAll(sel, roots)), null, api);
      }
      return new Cheerio([], null, api);
    };
    api.$ = $;
    $.root = () => new Cheerio([root], null, api);
    $.html = (x) => { if (x === undefined) return inner(root); const l = x instanceof Cheerio ? x._n() : [].concat(x); return l.map(outer).join(''); };
    $.xml = $.html;
    $.text = (x) => (x === undefined ? textOf(root) : (x instanceof Cheerio ? x._n() : [].concat(x)).map(textOf).join(''));
    $.contains = (a, b) => { for (let p = b.parent; p; p = p.parent) if (p === a) return true; return false; };
    $.parseHTML = (h) => parseHTML(h).children;
    $.fn = Cheerio.prototype;
    return $;
  }

  const cheerioLib = { load, default: { load }, Cheerio, version: 'lite' };
  cheerioLib.__esModule = true;

  // ---------------------------------------------------------------- htmlparser2
  class Parser {
    constructor(cbs, opts) { this.cbs = cbs || {}; this.opts = opts || {}; this.buf = ''; }
    write(s) { this.buf += s; }
    end(s) { if (s) this.buf += s; this.parseComplete(this.buf); }
    parseComplete(html) {
      const cb = this.cbs;
      const stack = [];
      const f = (name, ...a) => { if (typeof cb[name] === 'function') cb[name].apply(cb, a); };
      f('onparserinit', this);
      tokenize(String(html), {
        text(t) { f('ontext', decodeEntities(t)); },
        raw(t) { f('ontext', t); },
        comment(t) { f('oncomment', t); },
        open(name, attribs, selfClose) {
          f('onopentagname', name);
          Object.keys(attribs).forEach((k) => f('onattribute', k, attribs[k]));
          f('onopentag', name, attribs);
          if (selfClose || VOID.has(name)) f('onclosetag', name, true); else stack.push(name);
        },
        close(name) {
          const at = stack.lastIndexOf(name);
          if (at < 0) return;
          while (stack.length > at) f('onclosetag', stack.pop(), stack.length !== at);
        },
      });
      while (stack.length) f('onclosetag', stack.pop(), true);
      f('onend');
    }
    reset() { this.buf = ''; }
    // htmlparser2's own: the readnovelfull plugins (NovelFull, AllNovel…) call it while copying a chapter ("not a function")
    isVoidElement(name) { return VOID.has(String(name).toLowerCase()); }
    parseChunk(s) { this.write(s); }
    pause() {}
    resume() {}
  }
  const htmlparser2Lib = {
    Parser,
    parseDocument: (h) => parseHTML(h),
    parseDOM: (h) => parseHTML(h).children,
    DomUtils: { getText: (n) => (Array.isArray(n) ? n.map(textOf).join('') : textOf(n)), getInnerHTML: (n) => inner(n), getOuterHTML: (n) => outer(n) },
    decodeHTML: decodeEntities,
    __esModule: true,
  };

  // ---------------------------------------------------------------- dayjs (the small part plugins use)
  const MONTHS = ['January', 'February', 'March', 'April', 'May', 'June', 'July', 'August', 'September', 'October', 'November', 'December'];
  const DAYS = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday'];
  const pad = (n, l = 2) => String(n).padStart(l, '0');
  class Dayjs {
    constructor(d) { this.$d = d; }
    isValid() { return !isNaN(this.$d.getTime()); }
    valueOf() { return this.$d.getTime(); }
    unix() { return Math.floor(this.$d.getTime() / 1000); }
    toDate() { return new Date(this.$d.getTime()); }
    toISOString() { return this.$d.toISOString(); }
    toJSON() { return this.isValid() ? this.toISOString() : null; }
    toString() { return this.$d.toUTCString(); }
    locale() { return this; }
    clone() { return new Dayjs(new Date(this.$d.getTime())); }
    _shift(n, unit, sign) {
      const d = new Date(this.$d.getTime());
      n = Number(n) * sign;
      unit = String(unit || 'millisecond').replace(/s$/, '');
      switch (unit) {
        case 'year': case 'y': d.setFullYear(d.getFullYear() + n); break;
        case 'month': case 'M': d.setMonth(d.getMonth() + n); break;
        case 'week': case 'w': d.setDate(d.getDate() + 7 * n); break;
        case 'day': case 'd': case 'date': d.setDate(d.getDate() + n); break;
        case 'hour': case 'h': d.setHours(d.getHours() + n); break;
        case 'minute': case 'm': case 'min': d.setMinutes(d.getMinutes() + n); break;
        case 'second': case 's': d.setSeconds(d.getSeconds() + n); break;
        default: d.setTime(d.getTime() + n);
      }
      return new Dayjs(d);
    }
    add(n, u) { return this._shift(n, u, 1); }
    subtract(n, u) { return this._shift(n, u, -1); }
    startOf(u) { const d = new Date(this.$d.getTime()); u = String(u); if (u === 'day' || u === 'd') d.setHours(0, 0, 0, 0); else if (u === 'month') { d.setDate(1); d.setHours(0, 0, 0, 0); } else if (u === 'year') { d.setMonth(0, 1); d.setHours(0, 0, 0, 0); } else if (u === 'hour') d.setMinutes(0, 0, 0); return new Dayjs(d); }
    endOf(u) { return this.startOf(u).add(1, u).subtract(1, 'millisecond'); }
    diff(o, u) { const ms = this.valueOf() - dayjs(o).valueOf(); const m = { second: 1e3, minute: 6e4, hour: 36e5, day: 864e5, week: 6048e5 }[String(u || 'millisecond').replace(/s$/, '')] || 1; return Math.trunc(ms / m); }
    isBefore(o) { return this.valueOf() < dayjs(o).valueOf(); }
    isAfter(o) { return this.valueOf() > dayjs(o).valueOf(); }
    year() { return this.$d.getFullYear(); }
    month() { return this.$d.getMonth(); }
    date() { return this.$d.getDate(); }
    day() { return this.$d.getDay(); }
    hour() { return this.$d.getHours(); }
    minute() { return this.$d.getMinutes(); }
    second() { return this.$d.getSeconds(); }
    format(f) {
      if (!this.isValid()) return 'Invalid Date';
      const d = this.$d;
      f = f === undefined ? 'YYYY-MM-DDTHH:mm:ssZ' : f;
      const tz = -d.getTimezoneOffset();
      const z = (tz >= 0 ? '+' : '-') + pad(Math.floor(Math.abs(tz) / 60)) + ':' + pad(Math.abs(tz) % 60);
      const LOC = { LT: 'h:mm A', LTS: 'h:mm:ss A', L: 'MM/DD/YYYY', LL: 'MMMM D, YYYY', LLL: 'MMMM D, YYYY h:mm A', LLLL: 'dddd, MMMM D, YYYY h:mm A' };
      f = f.replace(/\[[^\]]*\]|LLLL|LLL|LTS|LL|LT|L/g, (t) => (t[0] === '[' ? t : LOC[t]));
      return f.replace(/\[([^\]]*)\]|YYYY|YY|MMMM|MMM|MM|M|DD|D|dddd|ddd|HH|H|hh|h|mm|m|ss|s|SSS|A|a|ZZ|Z/g, (t, lit) => {
        if (lit !== undefined) return lit;
        switch (t) {
          case 'YYYY': return pad(d.getFullYear(), 4);
          case 'YY': return pad(d.getFullYear() % 100);
          case 'MMMM': return MONTHS[d.getMonth()];
          case 'MMM': return MONTHS[d.getMonth()].slice(0, 3);
          case 'MM': return pad(d.getMonth() + 1);
          case 'M': return String(d.getMonth() + 1);
          case 'DD': return pad(d.getDate());
          case 'D': return String(d.getDate());
          case 'dddd': return DAYS[d.getDay()];
          case 'ddd': return DAYS[d.getDay()].slice(0, 3);
          case 'HH': return pad(d.getHours());
          case 'H': return String(d.getHours());
          case 'hh': return pad(d.getHours() % 12 || 12);
          case 'h': return String(d.getHours() % 12 || 12);
          case 'mm': return pad(d.getMinutes());
          case 'm': return String(d.getMinutes());
          case 'ss': return pad(d.getSeconds());
          case 's': return String(d.getSeconds());
          case 'SSS': return pad(d.getMilliseconds(), 3);
          case 'A': return d.getHours() < 12 ? 'AM' : 'PM';
          case 'a': return d.getHours() < 12 ? 'am' : 'pm';
          case 'ZZ': return z.replace(':', '');
          case 'Z': return z;
          default: return t;
        }
      });
    }
  }
  function parseWithFormat(s, fmt) {
    const names = [];
    const re = new RegExp('^\\s*' + String(fmt).replace(/[.*+?^${}()|[\]\\]/g, '\\$&').replace(/YYYY|YY|MMMM|MMM|MM|M|DD|D|HH|H|hh|h|mm|m|ss|s|A|a/g, (t) => {
      names.push(t);
      return t === 'MMMM' || t === 'MMM' ? '([A-Za-z]+)' : t === 'YYYY' ? '(\\d{4})' : t === 'A' || t === 'a' ? '([AaPp][Mm])' : '(\\d{1,2})';
    }), 'i');
    const m = re.exec(String(s));
    if (!m) return new Date(NaN);
    let y = new Date().getFullYear(), mo = 0, da = 1, h = 0, mi = 0, se = 0, pm = null;
    names.forEach((t, i) => {
      const v = m[i + 1];
      if (t === 'YYYY') y = +v; else if (t === 'YY') y = 2000 + +v;
      else if (t === 'MM' || t === 'M') mo = +v - 1;
      else if (t === 'MMMM' || t === 'MMM') mo = Math.max(0, MONTHS.findIndex((x) => x.toLowerCase().startsWith(v.toLowerCase().slice(0, 3))));
      else if (t === 'DD' || t === 'D') da = +v;
      else if (/^[Hh]/.test(t)) h = +v;
      else if (t === 'mm' || t === 'm') mi = +v;
      else if (t === 'ss' || t === 's') se = +v;
      else pm = /p/i.test(v);
    });
    if (pm === true && h < 12) h += 12;
    if (pm === false && h === 12) h = 0;
    return new Date(y, mo, da, h, mi, se);
  }
  function dayjs(input, fmt) {
    if (input instanceof Dayjs) return input.clone();
    if (input === undefined) return new Dayjs(new Date());
    if (input === null) return new Dayjs(new Date(NaN));
    if (typeof input === 'string' && typeof fmt === 'string') return new Dayjs(parseWithFormat(input, fmt));
    if (typeof input === 'string') {
      let d = new Date(input);
      if (isNaN(d.getTime())) {
        const m = /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?/.exec(input);
        if (m) d = new Date(+m[1], +m[2] - 1, +m[3], +(m[4] || 0), +(m[5] || 0), +(m[6] || 0));
      }
      return new Dayjs(d);
    }
    return new Dayjs(new Date(input));
  }
  dayjs.extend = function () { return dayjs; };
  dayjs.locale = function () { return 'en'; };
  dayjs.unix = (s) => new Dayjs(new Date(s * 1000));
  dayjs.isDayjs = (x) => x instanceof Dayjs;
  dayjs.default = dayjs;
  dayjs.__esModule = true;

  // ---------------------------------------------------------------- @libs
  const UA = 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36';
  const DEFAULT_COVER = 'https://github.com/LNReader/lnreader-plugins/blob/master/public/static/coverNotAvailable.webp?raw=true';

  class Response {
    constructor(r) {
      this.status = r.status;
      this.ok = r.status >= 200 && r.status < 300;
      this.statusText = r.statusText || '';
      this.url = r.url;
      this.headers = new Headers(r.headers || {});
      this._b = r.body;
      this.bodyUsed = false;
      this.redirected = false;
    }
    text() { this.bodyUsed = true; return Promise.resolve(this._b); }
    json() { this.bodyUsed = true; return Promise.resolve().then(() => JSON.parse(this._b)); }
    arrayBuffer() { return Promise.resolve(utf8Encode(this._b).buffer); }
    blob() { return Promise.resolve({ size: this._b.length, type: this.headers.get('content-type') || '', text: () => Promise.resolve(this._b) }); }
    clone() { return new Response({ status: this.status, url: this.url, headers: {}, body: this._b }); }
  }
  G.Response = Response;

  function fetchApi(input, init) {
    return new Promise((resolve, reject) => {
      try {
        init = init || {};
        let url = typeof input === 'string' ? input : input && input.url ? input.url : String(input);
        const headers = {};
        const h = init.headers;
        if (h instanceof Headers) h.forEach((v, k) => { headers[k] = v; });
        else if (Array.isArray(h)) h.forEach((p) => { headers[p[0]] = p[1]; });
        else if (h) Object.keys(h).forEach((k) => { headers[k] = h[k]; });
        const lower = Object.keys(headers).map((k) => k.toLowerCase());
        if (!lower.includes('user-agent')) headers['User-Agent'] = UA;
        let body = '';
        let b = init.body;
        if (b !== undefined && b !== null) {
          if (b instanceof URLSearchParams) { body = b.toString(); if (!lower.includes('content-type')) headers['Content-Type'] = 'application/x-www-form-urlencoded'; }
          else if (b instanceof FormData) { const sp = new URLSearchParams(); b.forEach((v, k) => sp.append(k, v)); body = sp.toString(); if (!lower.includes('content-type')) headers['Content-Type'] = 'application/x-www-form-urlencoded'; }
          else if (typeof b === 'string') body = b;
          else body = JSON.stringify(b);
        }
        if (init.referrer && !lower.includes('referer')) headers.Referer = init.referrer;
        const out = JSON.parse(__http(String(init.method || 'GET').toUpperCase(), url, JSON.stringify(headers), body));
        if (out.error) throw new TypeError('Network request failed: ' + out.error);
        resolve(new Response(out));
      } catch (e) { reject(e); }
    });
  }
  const fetchText = (url, init) => fetchApi(url, init).then((r) => r.text());
  const fetchFile = (url, init) => fetchApi(url, init).then((r) => r.text());
  const fetchProto = () => Promise.reject(new Error('fetchProto is not supported by this app'));
  G.fetch = fetchApi;

  const store = {};
  function makeStorage(prefix) {
    return {
      get(k) { return store[prefix + k]; },
      set(k, v) { store[prefix + k] = v; },
      delete(k) { delete store[prefix + k]; },
      clearAll() { Object.keys(store).forEach((k) => { if (k.startsWith(prefix)) delete store[k]; }); },
      getAllKeys() { return Object.keys(store).filter((k) => k.startsWith(prefix)).map((k) => k.slice(prefix.length)); },
      getItem(k) { const v = store[prefix + k]; return v === undefined ? null : v; },
      setItem(k, v) { store[prefix + k] = v; },
      removeItem(k) { delete store[prefix + k]; },
    };
  }
  G.localStorage = makeStorage('l:');
  G.sessionStorage = makeStorage('s:');

  const NovelStatus = { Unknown: 'Unknown', Ongoing: 'Ongoing', Completed: 'Completed', Licensed: 'Licensed', PublishingFinished: 'Publishing Finished', Cancelled: 'Cancelled', OnHiatus: 'On Hiatus' };
  const FilterTypes = { TextInput: 'Text', Picker: 'Picker', CheckboxGroup: 'Checkbox', Switch: 'Switch', ExcludableCheckboxGroup: 'XCheckbox' };

  const libs = {
    '@libs/fetch': { fetchApi, fetchText, fetchFile, fetchProto, __esModule: true },
    '@libs/novelStatus': { NovelStatus, __esModule: true },
    '@libs/defaultCover': { defaultCover: DEFAULT_COVER, __esModule: true },
    '@libs/filterInputs': { FilterTypes, __esModule: true },
    '@libs/isAbsoluteUrl': { isUrlAbsolute: (u) => /^([a-z][a-z0-9+.-]*:)?\/\//i.test(String(u)), __esModule: true },
    '@libs/storage': { storage: makeStorage('p:'), localStorage: G.localStorage, sessionStorage: G.sessionStorage, __esModule: true },
    '@libs/aes': { __esModule: true },
    '@/types/constants': { defaultCover: DEFAULT_COVER, defaultUserAgentString: UA, __esModule: true },
    cheerio: cheerioLib,
    htmlparser2: htmlparser2Lib,
    dayjs,
  };
  function req(name) {
    if (Object.prototype.hasOwnProperty.call(libs, name)) return libs[name];
    if (name.startsWith('dayjs/')) return dayjs;
    throw new Error('The plugin needs "' + name + '", which this app does not provide');
  }

  // ---------------------------------------------------------------- loading and calling plugins
  const plugins = {};
  G.__load = function (id, code) {
    const module = { exports: {} };
    const fn = new Function('require', 'module', 'exports', code);
    fn.call(module.exports, req, module, module.exports);
    const ex = module.exports;
    plugins[id] = ex.default || ex;
    const p = plugins[id];
    return JSON.stringify({ id: p.id, name: p.name, site: p.site, version: p.version, icon: p.icon, lang: p.lang });
  };
  G.__out = null;
  // Runs plugins[id][method](...args) and leaves the answer, as JSON text, in __out once the promise settles.
  G.__call = function (id, method, argsJson) {
    G.__out = null;
    try {
      const p = plugins[id];
      if (!p) throw new Error('Plugin not loaded');
      if (typeof p[method] !== 'function') throw new Error('The plugin has no ' + method);
      const args = JSON.parse(argsJson);
      // popularNovels gets the plugin's own default filters, like the LNReader app does.
      if (method === 'popularNovels' && args[1] && args[1].filters === '__default') {
        const f = {};
        Object.keys(p.filters || {}).forEach((k) => { f[k] = { type: p.filters[k].type, value: p.filters[k].value }; });
        args[1].filters = f;
      }
      Promise.resolve(p[method].apply(p, args)).then(
        (v) => { G.__out = JSON.stringify({ ok: v === undefined ? null : v }); },
        (e) => { G.__out = JSON.stringify({ err: String((e && e.message) || e) }); }
      );
    } catch (e) { G.__out = JSON.stringify({ err: String((e && e.message) || e) }); }
  };
  G.__has = function (id, method) { return !!(plugins[id] && typeof plugins[id][method] === 'function'); };
  G.__filters = function (id) { const p = plugins[id]; return JSON.stringify((p && p.filters) || null); };
  G.__test = { load, parseHTML, selectAll, dayjs, URL, outer, inner };
})(globalThis);
