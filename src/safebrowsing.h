/* Southstar — local phishing/malware blocklist and warning interstitial, implemented in rust/safebrowsing. */

#ifndef NS_SAFEBROWSING_H
#define NS_SAFEBROWSING_H

#include <glib.h>

G_BEGIN_DECLS

#define NS_UNSAFE_CONTINUE_SCHEME "southstar-unsafe-continue:"

gboolean ns_safebrowsing_blocked(const char *host);
void     ns_safebrowsing_allow_host(const char *host);
char    *ns_safebrowsing_interstitial(const char *url, const char *host);

G_END_DECLS

#endif
