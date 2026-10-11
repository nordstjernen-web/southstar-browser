/* Southstar — WebIDL brand checks and names for the native members of interfaces, implemented in rust/js-brand. */
#ifndef NS_JS_BRAND_H
#define NS_JS_BRAND_H

#include "ns_quickjs.h"

void ns_js_brand_node_interfaces(JSContext *ctx, JSClassID element_cid,
                                 JSClassID attr_cid);
void ns_js_name_engine_members(JSContext *ctx);

#endif
