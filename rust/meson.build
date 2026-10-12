cargo = find_program('cargo', version: '>=1.99')
rustc = find_program('rustc', version: '>=1.99')
cargo_build_py = files('../scripts/cargo-build.py')
cargo_python = import('python').find_installation()

rust_native_libs = run_command(
  cargo_python, cargo_build_py, 'native-libs', rustc, meson.project_source_root(),
  check: true,
).stdout().strip().split('\n')

southstar_rust_features = []
southstar_rust_link_deps = [sqlite3, libcrypto, libwebp, libwebpdemux, libuchardet, cairo, libpsl, pango, epoxy]
if host_machine.system() != 'windows'
  southstar_rust_link_deps += [meson.get_compiler('c').find_library('rt', required: false)]
endif
southstar_rust_env = {
  'NS_RUSTC_VERSION': rustc.version(),
  'NS_VERSION': meson.project_version(),
  'NS_BUILD_DATE': build_date,
  'NS_LEXBOR_VERSION': lexbor_version,
}
rust_cc = meson.get_compiler('c')
if have_libav
  av_layout_prefix = '''#include <stddef.h>
#include <errno.h>
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/log.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>'''
  av_layout_deps = [libavformat, libavcodec, libavutil, libswscale, libswresample]
  av_layout = []
  foreach member : [
    'AVFormatContext.nb_streams', 'AVFormatContext.streams',
    'AVFormatContext.duration', 'AVFormatContext.pb',
    'AVStream.codecpar', 'AVStream.time_base', 'AVStream.duration',
    'AVStream.avg_frame_rate', 'AVStream.r_frame_rate',
    'AVCodecParameters.codec_id', 'AVCodecParameters.width',
    'AVCodecParameters.height',
    'AVCodecContext.ch_layout', 'AVCodecContext.sample_rate',
    'AVCodecContext.sample_fmt',
    'AVIOContext.buffer', 'AVIOContext.eof_reached',
    'AVPacket.stream_index', 'AVPacket.pts', 'AVPacket.duration',
    'AVFrame.data', 'AVFrame.linesize', 'AVFrame.width', 'AVFrame.height',
    'AVFrame.format', 'AVFrame.pts', 'AVFrame.nb_samples',
    'AVFrame.ch_layout', 'AVFrame.best_effort_timestamp',
    'AVChannelLayout.nb_channels',
  ]
    parts = member.split('.')
    av_layout += member + '=' + rust_cc.compute_int(
      'offsetof(@0@, @1@)'.format(parts[0], parts[1]),
      prefix: av_layout_prefix, dependencies: av_layout_deps).to_string()
  endforeach
  foreach constant : [
    ['sizeof.AVChannelLayout', 'sizeof(AVChannelLayout)'],
    ['AVERROR_EAGAIN', 'AVERROR(EAGAIN)'], ['AVERROR_EOF', 'AVERROR_EOF'],
    ['AVMEDIA_TYPE_VIDEO', 'AVMEDIA_TYPE_VIDEO'],
    ['AVMEDIA_TYPE_AUDIO', 'AVMEDIA_TYPE_AUDIO'],
    ['AV_PIX_FMT_BGRA', 'AV_PIX_FMT_BGRA'],
    ['AV_SAMPLE_FMT_FLT', 'AV_SAMPLE_FMT_FLT'],
    ['AV_SAMPLE_FMT_NONE', 'AV_SAMPLE_FMT_NONE'],
    ['AV_LOG_QUIET', 'AV_LOG_QUIET'], ['SWS_BILINEAR', 'SWS_BILINEAR'],
    ['AVSEEK_FLAG_BACKWARD', 'AVSEEK_FLAG_BACKWARD'],
    ['AVSEEK_SIZE', 'AVSEEK_SIZE'],
    ['AV_CODEC_ID_NONE', 'AV_CODEC_ID_NONE'],
    ['AV_CODEC_ID_VP8', 'AV_CODEC_ID_VP8'], ['AV_CODEC_ID_VP9', 'AV_CODEC_ID_VP9'],
    ['AV_CODEC_ID_AV1', 'AV_CODEC_ID_AV1'], ['AV_CODEC_ID_H264', 'AV_CODEC_ID_H264'],
    ['AV_CODEC_ID_HEVC', 'AV_CODEC_ID_HEVC'],
    ['AV_CODEC_ID_MPEG4', 'AV_CODEC_ID_MPEG4'],
    ['AV_CODEC_ID_THEORA', 'AV_CODEC_ID_THEORA'],
    ['AV_CODEC_ID_OPUS', 'AV_CODEC_ID_OPUS'],
    ['AV_CODEC_ID_VORBIS', 'AV_CODEC_ID_VORBIS'],
    ['AV_CODEC_ID_AAC', 'AV_CODEC_ID_AAC'], ['AV_CODEC_ID_MP3', 'AV_CODEC_ID_MP3'],
    ['AV_CODEC_ID_FLAC', 'AV_CODEC_ID_FLAC'], ['AV_CODEC_ID_ALAC', 'AV_CODEC_ID_ALAC'],
    ['AV_CODEC_ID_AC3', 'AV_CODEC_ID_AC3'], ['AV_CODEC_ID_EAC3', 'AV_CODEC_ID_EAC3'],
  ]
    av_layout += constant[0] + '=' + rust_cc.compute_int(
      constant[1], prefix: av_layout_prefix, dependencies: av_layout_deps,
      low: -2147483648, high: 2147483647).to_string()
  endforeach
  southstar_rust_env += {'NS_AV_LAYOUT': ','.join(av_layout)}
endif
if have_avif
  avif_layout_prefix = '#include <stddef.h>\n#include <avif/avif.h>'
  avif_layout = []
  foreach member : [
    'avifDecoder.maxThreads', 'avifDecoder.imageSizeLimit',
    'avifDecoder.imageDimensionLimit', 'avifDecoder.strictFlags',
    'avifDecoder.image', 'avifImage.width', 'avifImage.height',
    'avifRGBImage.format', 'avifRGBImage.depth',
    'avifRGBImage.alphaPremultiplied', 'avifRGBImage.pixels',
    'avifRGBImage.rowBytes',
  ]
    parts = member.split('.')
    avif_layout += member + '=' + rust_cc.compute_int(
      'offsetof(@0@, @1@)'.format(parts[0], parts[1]),
      prefix: avif_layout_prefix, dependencies: [libavif]).to_string()
  endforeach
  foreach constant : [
    ['sizeof.avifRGBImage', 'sizeof(avifRGBImage)'],
    ['AVIF_RGB_FORMAT_BGRA', 'AVIF_RGB_FORMAT_BGRA'],
    ['AVIF_STRICT_DISABLED', 'AVIF_STRICT_DISABLED'],
    ['AVIF_RESULT_OK', 'AVIF_RESULT_OK'],
  ]
    avif_layout += constant[0] + '=' + rust_cc.compute_int(
      constant[1], prefix: avif_layout_prefix, dependencies: [libavif],
      low: -2147483648, high: 2147483647).to_string()
  endforeach
  southstar_rust_env += {'NS_AVIF_LAYOUT': ','.join(avif_layout)}
endif
if use_ns_pango
  southstar_rust_features += ['ns-pango']
endif
if have_egl and host_machine.system() not in ['darwin', 'windows']
  southstar_rust_features += ['egl']
endif
rust_sdl2 = dependency('sdl2', required: get_option('audio'))
if rust_sdl2.found()
  southstar_rust_features += ['sdl']
  southstar_rust_link_deps += [rust_sdl2.partial_dependency(link_args: true)]
endif
if have_fontconfig and host_machine.system() == 'windows'
  southstar_rust_features += ['fontconfig']
  southstar_rust_link_deps += [libfontconfig]
endif
if have_fontconfig
  southstar_rust_features += ['font-fontconfig']
  southstar_rust_link_deps += [libfontconfig]
endif
if have_freetype
  southstar_rust_features += ['freetype']
  southstar_rust_link_deps += [libfreetype]
endif
if have_pangoft2
  southstar_rust_features += ['pangoft2']
  southstar_rust_link_deps += [libpangoft2]
endif
if have_avif
  southstar_rust_features += ['avif']
  southstar_rust_link_deps += [libavif]
endif
if have_poppler
  southstar_rust_features += ['poppler']
  southstar_rust_link_deps += [libpoppler]
endif
if have_seccomp
  southstar_rust_features += ['seccomp']
  southstar_rust_link_deps += [libseccomp]
endif
if have_enchant
  southstar_rust_features += ['enchant']
  southstar_rust_link_deps += [libenchant]
endif
if have_libav
  southstar_rust_features += ['libav']
endif
if have_audio_native_vorbis
  southstar_rust_features += ['audio-native-vorbis']
endif
if have_audio_native_opus
  southstar_rust_features += ['audio-native-opus']
endif
if have_http_brotli
  southstar_rust_features += ['http-brotli']
endif
if have_http_zstd
  southstar_rust_features += ['http-zstd']
endif
southstar_rust_link_deps += http_backend_deps
if have_webgpu
  southstar_rust_features += ['webgpu']
  southstar_rust_link_deps += [webgpu_dep]
endif
if have_wasm
  southstar_rust_features += ['wasm']
endif
if have_woff2
  southstar_rust_features += ['woff2']
  southstar_rust_link_deps += [libbrotlidec_fonts]
endif

southstar_rust_lib = custom_target(
  'southstar-rust',
  output: 'libsouthstar_ffi.a',
  depfile: 'libsouthstar_ffi.a.d',
  command: [
    cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
    meson.current_build_dir() / 'cargo-target',
    ns_is_release ? 'release' : 'dev',
    'southstar-static', '@OUTPUT@', '@DEPFILE@',
  ] + southstar_rust_features,
  env: southstar_rust_env,
  console: true,
  build_by_default: true,
)

southstar_rust_dep = declare_dependency(
  link_with: southstar_rust_lib,
  link_args: rust_native_libs,
  dependencies: southstar_rust_link_deps,
)

if have_gtk
  southstar_shell_rust_lib = custom_target(
    'southstar-shell-rust',
    output: 'libsouthstar_shell.a',
    depfile: 'libsouthstar_shell.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_is_release ? 'release' : 'dev',
      'southstar-shell', '@OUTPUT@', '@DEPFILE@',
    ] + southstar_rust_features,
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
    depends: southstar_rust_lib,
  )

  southstar_shell_rust_dep = declare_dependency(
    link_with: southstar_shell_rust_lib,
    link_args: rust_native_libs,
    dependencies: southstar_rust_link_deps + [gtk4_dep],
  )
endif

if host_machine.system() == 'windows'
  southstar_launcher_rust_lib = custom_target(
    'southstar-launcher-rust',
    output: 'libsouthstar_launcher.a',
    depfile: 'libsouthstar_launcher.a.d',
    command: [
      cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
      meson.current_build_dir() / 'cargo-target',
      ns_is_release ? 'release' : 'dev',
      'southstar-launcher', '@OUTPUT@', '@DEPFILE@',
    ],
    env: southstar_rust_env,
    console: true,
    build_by_default: true,
    depends: southstar_rust_lib,
  )
endif

southstar_helper_rust_features = have_seccomp ? ['seccomp'] : []
southstar_helper_rust_lib = custom_target(
  'southstar-helper-rust',
  output: 'libsouthstar_helper_ffi.a',
  depfile: 'libsouthstar_helper_ffi.a.d',
  command: [
    cargo_python, cargo_build_py, 'build', cargo, meson.project_source_root(),
    meson.current_build_dir() / 'cargo-target',
    ns_is_release ? 'release' : 'dev',
    'southstar-helper-ffi', '@OUTPUT@', '@DEPFILE@',
  ] + southstar_helper_rust_features,
  env: southstar_rust_env,
  console: true,
  build_by_default: true,
)

southstar_helper_rust_dep = declare_dependency(
  link_with: southstar_helper_rust_lib,
  link_args: rust_native_libs,
  dependencies: [glib] + http_backend_deps + (have_seccomp ? [libseccomp] : []),
)

