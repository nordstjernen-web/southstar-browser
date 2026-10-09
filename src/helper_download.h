/* Southstar — media downloads for the helper processes, implemented in rust/helper-ffi.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_HELPER_DOWNLOAD_H
#define NS_HELPER_DOWNLOAD_H

void ns_helper_net_init(void);
int  ns_helper_download(const char *url, const char *path, unsigned long long max_bytes);

#endif
