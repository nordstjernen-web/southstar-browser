(function(urlParts, urlSet){
 return function(u, proto){
  var URLp = URL.prototype;
  if (!URLp.__ndReady) {
    function nm(fn, n){ try { Object.defineProperty(fn, 'name', { value: n, configurable: true }); } catch(e) {} return fn; }
    function st(o){ var d = o !== null && typeof o === 'object' ? o.__nd : undefined; if (!d) throw new TypeError('Illegal invocation'); return d; }
    function gettr(name){ return function(){ return st(this)[name]; }; }
    function setComp(comp){ return function(v){ var d = st(this); var p = urlSet(d.href, comp, String(v)); if (p) { this.__nd = p; this.__ndSync(); } }; }
    function accessor(name, setter){ Object.defineProperty(URLp, name, { configurable: true, enumerable: true, get: nm(gettr(name), 'get ' + name), set: setter ? nm(setter, 'set ' + name) : undefined }); }
    accessor('href', function(v){ st(this); var p = urlParts(String(v)); if (!p) throw new TypeError('Invalid URL'); this.__nd = p; this.__ndSync(); });
    accessor('origin', undefined);
    ['protocol','username','password','host','hostname','port','pathname','search','hash'].forEach(function(n){ accessor(n, setComp(n)); });
    Object.defineProperty(URLp, 'searchParams', { configurable: true, enumerable: true, get: nm(function(){ st(this); return this.__ndSP; }, 'get searchParams') });
    function method(name, fn){ Object.defineProperty(URLp, name, { configurable: true, enumerable: true, writable: true, value: nm(fn, name) }); }
    method('toString', function(){ return st(this).href; });
    method('toJSON', function(){ return st(this).href; });
    URLp.__ndSync = function(){ try { var sp = new URLSearchParams(this.__nd.search); if (this.__ndSP) { this.__ndSP.__ndPairs = sp.__ndPairs; } else { sp.__ndOwner = this; this.__ndSP = sp; } } catch(e) {} };
    URLp.__ndSetSearchRaw = function(v){ var p = urlSet(this.__nd.href, 'search', String(v)); if (p) { this.__nd = p; this.__ndSync(); } };
    try { Object.defineProperty(URLp, Symbol.toStringTag, { value: 'URL', configurable: true }); } catch(e) {}
    URLp.__ndReady = true;
  }
  var nd = { href: u.href, origin: u.origin, protocol: u.protocol, username: u.username, password: u.password, host: u.host, hostname: u.hostname, port: u.port, pathname: (u.pathname == null ? '' : u.pathname), search: u.search || '', hash: u.hash || '' };
  var inst = Object.create(proto && (proto === URLp || URLp.isPrototypeOf(proto)) ? proto : URLp);
  inst.__nd = nd;
  inst.__ndSync();
  return inst;
 };
})
