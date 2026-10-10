(function(init, proto){
 var USPp = URLSearchParams.prototype;
 if (!USPp.__ndReady) {
   function nm(fn,n){ try { Object.defineProperty(fn,'name',{ value:n, configurable:true }); } catch(e){} return fn; }
   function fenc(s){ return encodeURIComponent(String(s)).replace(/[!~'()]/g,function(c){return '%'+c.charCodeAt(0).toString(16).toUpperCase();}).replace(/%20/g,'+'); }
   function chk(o){ var p = o !== null && typeof o === 'object' ? o.__ndPairs : undefined; if (!Array.isArray(p)) throw new TypeError('Illegal invocation'); return p; }
   function meth(name, fn){ Object.defineProperty(USPp, name, { configurable:true, enumerable:true, writable:true, value:nm(fn,name) }); }
   function req(n,c){ if (c < n) throw new TypeError(n+' arguments required'); }
   USPp.__ndNotify = function(){ var ow = this.__ndOwner; if (ow && ow.__ndSetSearchRaw) { var s = this.toString(); ow.__ndSetSearchRaw(s ? '?' + s : ''); } };
   meth('toString', function(){ return chk(this).map(function(p){return fenc(p[0])+'='+fenc(p[1]);}).join('&'); });
   meth('get', function(k){ var P = chk(this); req(1,arguments.length); k=String(k); for (var i=0;i<P.length;i++) if(P[i][0]===k) return P[i][1]; return null; });
   meth('getAll', function(k){ var P = chk(this); req(1,arguments.length); k=String(k); var r=[]; for (var i=0;i<P.length;i++) if(P[i][0]===k) r.push(P[i][1]); return r; });
   meth('has', function(k){ var P = chk(this); req(1,arguments.length); k=String(k); var hv=arguments.length>1&&arguments[1]!==undefined; var vv=hv?String(arguments[1]):null; for (var i=0;i<P.length;i++) if(P[i][0]===k&&(!hv||P[i][1]===vv)) return true; return false; });
   meth('set', function(k,v){ var P = chk(this); req(2,arguments.length); k=String(k); v=String(v); var found=false; var out=[]; for (var i=0;i<P.length;i++){ if(P[i][0]===k){ if(!found){out.push([k,v]);found=true;} } else out.push(P[i]); } if(!found) out.push([k,v]); this.__ndPairs=out; this.__ndNotify(); });
   meth('append', function(k,v){ var P = chk(this); req(2,arguments.length); P.push([String(k),String(v)]); this.__ndNotify(); });
   meth('delete', function(k){ var P = chk(this); req(1,arguments.length); k=String(k); var hv=arguments.length>1&&arguments[1]!==undefined; var vv=hv?String(arguments[1]):null; this.__ndPairs=P.filter(function(p){return !(p[0]===k&&(!hv||p[1]===vv));}); this.__ndNotify(); });
   meth('sort', function(){ var P = chk(this); P.sort(function(a,b){return a[0]<b[0]?-1:a[0]>b[0]?1:0;}); this.__ndNotify(); });
   meth('forEach', function(cb){ var P = chk(this); req(1,arguments.length); if (typeof cb !== 'function') throw new TypeError("Failed to execute 'forEach' on 'URLSearchParams': parameter 1 is not of type 'Function'."); var th=arguments[1]; for (var i=0;i<P.length;i++) cb.call(th,P[i][1],P[i][0],this); });
   function* walk(o, kind){ for (var i=0;i<o.__ndPairs.length;i++){ var e=o.__ndPairs[i]; yield kind===0?[e[0],e[1]]:kind===1?e[0]:e[1]; } }
   meth('keys', function(){ chk(this); return walk(this, 1); });
   meth('values', function(){ chk(this); return walk(this, 2); });
   meth('entries', function(){ chk(this); return walk(this, 0); });
   Object.defineProperty(USPp, Symbol.iterator, { configurable:true, writable:true, value:USPp.entries });
   Object.defineProperty(USPp,'size',{ configurable:true, enumerable:true, get:nm(function(){ return chk(this).length; },'get size') });
   try { Object.defineProperty(USPp,Symbol.toStringTag,{ value:'URLSearchParams', configurable:true }); } catch(e){}
   USPp.__ndReady = true;
 }
 var pairs=[];
 function usv(s){s=String(s);var o='',i;for(i=0;i<s.length;i++){var c=s.charCodeAt(i);if(c>=0xD800&&c<=0xDBFF){var d=i+1<s.length?s.charCodeAt(i+1):0;if(d>=0xDC00&&d<=0xDFFF){o+=s[i]+s[i+1];i++;}else o+='�';}else if(c>=0xDC00&&c<=0xDFFF){o+='�';}else{o+=s[i];}}return o;}
 function add(k,v){pairs.push([usv(k),usv(v)]);}
 function pdecode(s){
   s=String(s).replace(/\+/g,' ');
   var out=[];
   for (var i=0;i<s.length;){
     if (s.charCodeAt(i)===37 && i+2<s.length && /^[0-9a-fA-F]{2}$/.test(s.substr(i+1,2))){
       out.push(parseInt(s.substr(i+1,2),16)); i+=3; continue;
     }
     var cp=s.codePointAt(i); i+=cp>65535?2:1;
     if (cp<128) out.push(cp);
     else if (cp<2048) out.push(192|(cp>>6),128|(cp&63));
     else if (cp<65536) out.push(224|(cp>>12),128|((cp>>6)&63),128|(cp&63));
     else out.push(240|(cp>>18),128|((cp>>12)&63),128|((cp>>6)&63),128|(cp&63));
   }
   return new TextDecoder('utf-8').decode(new Uint8Array(out));
 }
 function parse(q){
   if (q && q[0]==='?') q=q.slice(1);
   if (!q) return;
   var parts=String(q).split('&');
   for (var i=0;i<parts.length;i++){
     if (!parts[i]) continue;
     var eq=parts[i].indexOf('=');
     var k=eq<0?parts[i]:parts[i].slice(0,eq);
     var v=eq<0?'':parts[i].slice(eq+1);
     add(pdecode(k),pdecode(v));
   }
 }
 if (init == null) {
 } else if (typeof init === 'string') {
   parse(init);
 } else if (typeof init === 'object' && typeof Symbol !== 'undefined' && typeof init[Symbol.iterator] === 'function') {
   var it=init[Symbol.iterator](), step;
   while(!(step=it.next()).done){
     var p=step.value;
     if (p == null || typeof p[Symbol.iterator] !== 'function')
       throw new TypeError('Query pair must be iterable');
     var pa=[]; var pit=p[Symbol.iterator]();
     for (var ps;!(ps=pit.next()).done;) pa.push(ps.value);
     if (pa.length !== 2) throw new TypeError('Each query pair must be an iterable [name, value] tuple');
     add(pa[0],pa[1]);
   }
 } else if (typeof init === 'object') {
   var ks=Object.keys(init);
   var rec=new Map();
   for (var oi=0;oi<ks.length;oi++) rec.set(usv(ks[oi]),usv(init[ks[oi]]));
   rec.forEach(function(v,k){pairs.push([k,v]);});
 } else {
   parse(String(init));
 }
 var o = Object.create(proto && (proto === USPp || USPp.isPrototypeOf(proto)) ? proto : USPp);
 o.__ndPairs = pairs;
 o.__ndOwner = null;
 return o;
})
