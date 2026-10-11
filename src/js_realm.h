/* Southstar — native ShadowRealm and the realm-global setup every window and frame shares, implemented in rust/js-realm. */
#ifndef NS_JS_REALM_H
#define NS_JS_REALM_H

#include "ns_quickjs.h"

void ns_js_realm_install(JSContext *ctx, JSValueConst global);
void ns_hide_shared_array_buffer(JSContext *ctx, JSValueConst global);

#endif
