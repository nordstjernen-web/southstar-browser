/* Southstar — inline video decoding (pl_mpeg MPEG-1, libav containers), implemented in rust/video.
 * Copyright 2026 Andreas Røsdal
 * SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later
 */

#ifndef NS_VIDEO_DECODE_H
#define NS_VIDEO_DECODE_H

#include <glib.h>

#include "texture.h"

G_BEGIN_DECLS

typedef struct ns_video_player ns_video_player;

ns_video_player *ns_video_player_new(const guint8 *bytes, gsize len);
void             ns_video_player_free(ns_video_player *player);

int    ns_video_player_width(const ns_video_player *player);
int    ns_video_player_height(const ns_video_player *player);
double ns_video_player_duration(const ns_video_player *player);
gboolean ns_video_codec_available(const char *codec);

ns_texture *ns_video_player_frame_at(ns_video_player *player, double seconds,
                                     gboolean loop, gboolean *out_ended);


G_END_DECLS

#endif
