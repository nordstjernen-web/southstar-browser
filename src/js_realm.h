/* Southstar — native ShadowRealm for the QuickJS engine, implemented in rust/js-realm. */
#ifndef NS_JS_REALM_H
#define NS_JS_REALM_H

#include "ns_quickjs.h"

void ns_js_realm_install(JSContext *ctx, JSValueConst global);

#endif
