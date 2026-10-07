/* Southstar — native Temporal date/time API for the QuickJS engine, implemented in rust/js-temporal. */
#ifndef NS_JS_DATE_H
#define NS_JS_DATE_H

#include "ns_quickjs.h"

void ns_js_temporal_install(JSContext *ctx, JSValueConst global);

#endif
