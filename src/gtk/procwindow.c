/* Southstar — GTK tabbed process-per-tab browser shell (IPC renderer). */

#include "procwindow.h"
#include <glib/gstdio.h>
#include "icons.h"
#include "procview.h"
#include "shellwin.h"
#include "titlerow.h"
#include "i18n.h"
#include "rproc_inproc.h"
#include "bookmarks.h"
#include "cache.h"
#include "config.h"
#include "history.h"
#include "libsouthstar.h"
#include "net.h"
#include "security.h"
#include "version.h"
#include "image.h"
#ifdef __APPLE__
#include "macos_dock.h"
#endif

#include <locale.h>
#include <stdlib.h>
#include <string.h>

#define NS_PROC_APP_ID "org.southstar.WebBrowser"
#define NS_FULLSCREEN_NOTICE_SECONDS 5

static int g_initial_win_w;
static int g_initial_win_h;

void
ns_procapp_set_window_size(int width, int height)
{
    g_initial_win_w = width;
    g_initial_win_h = height;
}

typedef struct {
    GtkApplication *app;
    GtkWidget      *window;
    GtkWidget      *header;
    GtkWidget      *toolbar;
    GtkWidget      *notebook;
    GtkWidget      *tabstrip;
    GtkWidget      *newtab_btn;
    GtkWidget      *address;
    gboolean        address_click_focuses;
    GtkWidget      *zoom_button;
    GtkWidget      *back;
    GtkWidget      *forward;
    GtkWidget      *reload;
    gboolean        loading;
    GtkWidget      *status;
    char           *status_base;
    guint           status_timer;
    gulong          theme_watch[2];
    gboolean        webgl_active;
    gboolean        element_fullscreen;
    NsProcView     *fullscreen_view;
    GtkWidget      *fullscreen_notice;
    guint           fullscreen_notice_timer;
    GtkWidget      *bookmarks_button;
    ns_bookmarks   *bookmarks;
    char           *session_path;
    guint           session_timer;
} ProcWindow;

static const char *
ns_brand_versioned(void)
{
    static char brand[128];
    if (!brand[0])
        g_snprintf(brand, sizeof brand, "%s %s",
                   ns_i18n("Southstar Browser"), NS_VERSION);
    return brand;
}

static void
procwindow_free(gpointer data)
{
    ProcWindow *pw = data;
    if (pw->session_timer)
        g_source_remove(pw->session_timer);
    if (pw->status_timer)
        g_source_remove(pw->status_timer);
    if (pw->fullscreen_notice_timer)
        g_source_remove(pw->fullscreen_notice_timer);
    GtkSettings *settings = gtk_settings_get_default();
    for (int i = 0; settings && i < 2; i++)
        if (pw->theme_watch[i])
            g_signal_handler_disconnect(settings, pw->theme_watch[i]);
    g_free(pw->session_path);
    g_free(pw->status_base);
    if (pw->bookmarks)
        ns_bookmarks_free(pw->bookmarks);
    g_free(pw);
}

static void
install_icon_search_paths(void)
{
    GdkDisplay *display = gdk_display_get_default();
    if (!display)
        return;
    GtkIconTheme *theme = gtk_icon_theme_get_for_display(display);
    if (!theme)
        return;
    gtk_icon_theme_add_resource_path(theme, "/org/southstar/WebBrowser/icons");
    const char *exe = ns_app_self_exe();
    if (!exe)
        return;
    char *dir = g_path_get_dirname(exe);
    const char *rel[] = { "share/icons",      "../share/icons",
                          "data/icons",       "../data/icons",
                          "../../data/icons", "../../../data/icons",
                          "../../../../data/icons", NULL };
    for (int i = 0; rel[i]; i++) {
        char *p = g_build_filename(dir, rel[i], NULL);
        gtk_icon_theme_add_search_path(theme, p);
        g_free(p);
    }
    g_free(dir);
}

static void
install_chrome_css(void)
{
    GdkDisplay *display = gdk_display_get_default();
    if (!display)
        return;
    GtkCssProvider *p = gtk_css_provider_new();
    gtk_css_provider_load_from_string(
        p,
        "@define-color ns_strip_base mix(@theme_bg_color, @theme_fg_color, .07);"
        "@define-color ns_strip mix(@ns_strip_base, #3a5a9a, 0.05);"
        "@define-color ns_surface mix(@theme_base_color, #3a5a9a, 0.03);"
        "@define-color ns_accent @theme_selected_bg_color;"
        "@define-color ns_private #8b5cf6;"
        "@define-color ns_private_surface mix(@ns_surface, #8b5cf6, 0.09);"
        "headerbar, headerbar:backdrop {"
        "  min-height: 0;"
        "  padding: 0;"
        "  border-width: 0;"
        "  box-shadow: none;"
        "  background: @ns_strip;"
        "}"
        "headerbar > windowhandle { min-height: 42px; }"
        "headerbar > windowhandle > box { padding: 0 8px; border-spacing: 0; }"
        "headerbar > windowhandle > box > box.start,"
        "headerbar > windowhandle > box > box.end { margin: 0; }"
        ".ns-brand-title {"
        "  font-size: 13px;"
        "  font-weight: bold;"
        "  letter-spacing: 0.02em;"
        "  color: alpha(currentColor, 0.72);"
        "}"
        "headerbar:backdrop .ns-brand-title {"
        "  color: alpha(currentColor, 0.5);"
        "}"
        "headerbar windowcontrols button {"
        "  min-width: 24px;"
        "  min-height: 24px;"
        "  margin: 0 3px;"
        "  padding: 0;"
        "  border-radius: 999px;"
        "  background-color: alpha(currentColor, 0.07);"
        "}"
        "headerbar windowcontrols button:hover {"
        "  background-color: alpha(currentColor, 0.15);"
        "}"
        ".ns-tabstrip { padding: 0; }"
        ".ns-tab {"
        "  margin-top: 7px;"
        "  padding: 0 5px 0 12px;"
        "  border-radius: 10px 10px 0 0;"
        "  transition: background-color 150ms ease-out;"
        "}"
        ".ns-tab:hover {"
        "  background-color: alpha(currentColor, 0.06);"
        "}"
        ".ns-tab.ns-tab-active { background-color: @ns_surface; }"
        ".ns-tab.ns-tab-private.ns-tab-active {"
        "  background-color: @ns_private_surface;"
        "}"
        ".ns-tab > button.ns-tab-label,"
        ".ns-tab > button.ns-tab-label:hover,"
        ".ns-tab > button.ns-tab-label:active {"
        "  min-height: 35px;"
        "  margin: 0;"
        "  padding: 0 6px 0 0;"
        "  border: none;"
        "  background: none;"
        "  box-shadow: none;"
        "  color: alpha(currentColor, 0.7);"
        "  font-size: 13px;"
        "}"
        ".ns-tab:hover > button.ns-tab-label,"
        ".ns-tab.ns-tab-active > button.ns-tab-label {"
        "  color: @theme_fg_color;"
        "}"
        ".ns-tab > button.ns-tab-label image {"
        "  -gtk-icon-size: 16px;"
        "}"
        ".ns-tab.ns-tab-private > button.ns-tab-label image {"
        "  color: @ns_private;"
        "}"
        ".ns-tab > button.ns-tab-close {"
        "  min-width: 22px;"
        "  min-height: 22px;"
        "  margin: 0;"
        "  padding: 0;"
        "  border: none;"
        "  border-radius: 999px;"
        "  background: none;"
        "  box-shadow: none;"
        "  -gtk-icon-size: 16px;"
        "  opacity: 0.5;"
        "  transition: opacity 120ms ease-out, background-color 120ms ease-out;"
        "}"
        ".ns-tab:hover > button.ns-tab-close,"
        ".ns-tab.ns-tab-active > button.ns-tab-close {"
        "  opacity: 0.85;"
        "}"
        ".ns-tab > button.ns-tab-close:hover {"
        "  opacity: 1;"
        "  background-color: alpha(currentColor, 0.12);"
        "}"
        ".ns-tabstrip > button.ns-newtab {"
        "  min-width: 30px;"
        "  min-height: 30px;"
        "  margin: 7px 0 0 6px;"
        "  padding: 0;"
        "  border: none;"
        "  border-radius: 999px;"
        "  background: none;"
        "  box-shadow: none;"
        "  -gtk-icon-size: 16px;"
        "}"
        ".ns-tabstrip > button.ns-newtab:hover {"
        "  background-color: alpha(currentColor, 0.09);"
        "}"
        ".ns-toolbar {"
        "  background-color: @ns_surface;"
        "  border-bottom: 1px solid alpha(currentColor, 0.1);"
        "  padding: 5px 8px;"
        "  transition: background-color 200ms ease-out;"
        "}"
        ".ns-toolbar.ns-private {"
        "  background-color: @ns_private_surface;"
        "}"
        ".ns-toolbar button.ns-nav-button,"
        ".ns-toolbar menubutton.ns-nav-button > button {"
        "  min-width: 34px;"
        "  min-height: 34px;"
        "  margin: 0;"
        "  padding: 0;"
        "  border: none;"
        "  border-radius: 999px;"
        "  background: none;"
        "  box-shadow: none;"
        "  color: alpha(currentColor, 0.85);"
        "  -gtk-icon-size: 16px;"
        "  transition: background-color 120ms ease-out;"
        "}"
        ".ns-toolbar button.ns-nav-button:hover,"
        ".ns-toolbar menubutton.ns-nav-button > button:hover {"
        "  background-color: alpha(currentColor, 0.08);"
        "  color: @theme_fg_color;"
        "}"
        ".ns-toolbar button.ns-nav-button:active,"
        ".ns-toolbar menubutton.ns-nav-button > button:active,"
        ".ns-toolbar menubutton.ns-nav-button > button:checked {"
        "  background-color: alpha(currentColor, 0.14);"
        "}"
        ".ns-toolbar button.ns-nav-button.ns-logo {"
        "  margin-left: 4px;"
        "  -gtk-icon-size: 22px;"
        "}"
        ".ns-toolbar button.ns-nav-button:disabled {"
        "  background: none;"
        "  color: alpha(currentColor, 0.3);"
        "}"
        ".ns-toolbar entry.ns-address {"
        "  min-height: 34px;"
        "  margin: 0 8px;"
        "  padding: 0 6px 0 12px;"
        "  border: 1px solid transparent;"
        "  border-radius: 999px;"
        "  background-color: alpha(currentColor, 0.065);"
        "  box-shadow: none;"
        "  outline: none;"
        "  color: inherit;"
        "  font-size: 14px;"
        "  transition: background-color 150ms ease-out,"
        "              border-color 150ms ease-out,"
        "              box-shadow 150ms ease-out;"
        "}"
        ".ns-toolbar entry.ns-address:hover {"
        "  background-color: alpha(currentColor, 0.09);"
        "}"
        ".ns-toolbar entry.ns-address:focus-within {"
        "  background-color: @theme_base_color;"
        "  border-color: @ns_accent;"
        "  box-shadow: 0 0 0 3px alpha(@ns_accent, 0.22);"
        "  outline: none;"
        "}"
        ".ns-toolbar entry.ns-address > text {"
        "  min-height: 0;"
        "  padding: 0;"
        "}"
        ".ns-toolbar entry.ns-address > image {"
        "  -gtk-icon-size: 16px;"
        "  color: alpha(currentColor, 0.65);"
        "}"
        ".ns-toolbar entry.ns-address > image.left {"
        "  margin-right: 10px;"
        "}"
        ".ns-toolbar entry.ns-address > image.right {"
        "  padding: 5px;"
        "  margin-left: 6px;"
        "  border-radius: 999px;"
        "}"
        ".ns-toolbar entry.ns-address > image.right:hover {"
        "  color: @theme_fg_color;"
        "  background-color: alpha(currentColor, 0.1);"
        "}"
        ".ns-toolbar entry.ns-address.ns-bookmarked > image.right {"
        "  color: @ns_accent;"
        "}"
        ".ns-toolbar entry.ns-address.ns-insecure > image.left {"
        "  color: @error_color;"
        "}"
        ".ns-toolbar button.ns-zoom {"
        "  min-height: 26px;"
        "  min-width: 0;"
        "  padding: 0 10px;"
        "  margin: 0 4px 0 0;"
        "  border: none;"
        "  border-radius: 999px;"
        "  background-color: alpha(@ns_accent, 0.14);"
        "  box-shadow: none;"
        "  color: @ns_accent;"
        "  font-size: 12px;"
        "  font-weight: bold;"
        "}"
        ".ns-toolbar button.ns-zoom:hover {"
        "  background-color: alpha(@ns_accent, 0.22);"
        "}"
        ".ns-procstatus {"
        "  margin: 0 0 8px 8px;"
        "  padding: 5px 12px;"
        "  border-radius: 999px;"
        "  border: 1px solid alpha(currentColor, 0.12);"
        "  background-color: @ns_surface;"
        "  box-shadow: 0 2px 8px alpha(black, 0.14);"
        "  font-size: 12px;"
        "}"
        ".ns-fullscreen-notice {"
        "  margin-top: 28px;"
        "  padding: 12px 24px;"
        "  border-radius: 999px;"
        "  background: alpha(black, 0.85);"
        "  color: white;"
        "  border: 1px solid alpha(white, 0.2);"
        "  box-shadow: 0 6px 24px alpha(black, 0.3);"
        "}"
        "frame.ns-findbar {"
        "  border: 1px solid alpha(currentColor, 0.12);"
        "  border-radius: 14px;"
        "  background-color: @ns_surface;"
        "  box-shadow: 0 6px 20px alpha(black, 0.16);"
        "}"
        "frame.ns-findbar > .toolbar {"
        "  padding: 6px;"
        "  background: none;"
        "}"
        "frame.ns-findbar entry { border-radius: 999px; }"
        "frame.ns-findbar button {"
        "  min-width: 30px;"
        "  min-height: 30px;"
        "  padding: 0;"
        "  border: none;"
        "  border-radius: 999px;"
        "  background: none;"
        "  box-shadow: none;"
        "}"
        "frame.ns-findbar button:hover {"
        "  background-color: alpha(currentColor, 0.09);"
        "}");
    gtk_style_context_add_provider_for_display(
        display, GTK_STYLE_PROVIDER(p),
        GTK_STYLE_PROVIDER_PRIORITY_APPLICATION);
    g_object_unref(p);
}

static void
set_accessible_label(GtkWidget *w, const char *label)
{
    gtk_accessible_update_property(GTK_ACCESSIBLE(w),
                                   GTK_ACCESSIBLE_PROPERTY_LABEL, label, -1);
}

static void
toggle_css_class(GtkWidget *w, const char *css_class, gboolean on)
{
    if (on)
        gtk_widget_add_css_class(w, css_class);
    else
        gtk_widget_remove_css_class(w, css_class);
}

static GtkWidget *
toolbar_button(const char *icon, const char *tooltip, GCallback cb,
               gpointer data)
{
    GtkWidget *b = ns_icon_button_new(icon);
    gtk_button_set_has_frame(GTK_BUTTON(b), FALSE);
    gtk_widget_add_css_class(b, "ns-nav-button");
    gtk_widget_set_valign(b, GTK_ALIGN_CENTER);
    gtk_widget_set_tooltip_text(b, tooltip);
    set_accessible_label(b, tooltip);
    g_signal_connect(b, "clicked", cb, data);
    return b;
}

static NsProcView *
view_for_page(GtkWidget *page)
{
    return page ? g_object_get_data(G_OBJECT(page), "ns-proc-view") : NULL;
}

static NsProcView *
current_view(ProcWindow *pw)
{
    int idx = gtk_notebook_get_current_page(GTK_NOTEBOOK(pw->notebook));
    if (idx < 0)
        return NULL;
    return view_for_page(
        gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), idx));
}

static char *
normalize_url(const char *input)
{
    char *trimmed = g_strstrip(g_strdup(input ? input : ""));
    if (!*trimmed)
        return trimmed;
    if (g_str_has_prefix(trimmed, "about:") ||
        g_str_has_prefix(trimmed, "file:") ||
        g_str_has_prefix(trimmed, "data:") ||
        g_str_has_prefix(trimmed, "view-source:") || strstr(trimmed, "://"))
        return trimmed;
    char *local = ns_url_from_local_path(trimmed);
    if (local) {
        g_free(trimmed);
        return local;
    }
    if (ns_address_is_search(trimmed)) {
        char *out = ns_search_url_for(trimmed);
        g_free(trimmed);
        return out;
    }
    char *out = g_strconcat("https://", trimmed, NULL);
    g_free(trimmed);
    return out;
}

static void
set_loading_ui(ProcWindow *pw, gboolean loading)
{
    pw->loading = loading;
    const char *tip = loading ? ns_i18n("Stop loading this page")
                              : ns_i18n("Reload this page");
    ns_icon_button_set(GTK_BUTTON(pw->reload),
                       loading ? "southstar-stop-symbolic"
                               : "southstar-reload-symbolic");
    gtk_widget_set_tooltip_text(pw->reload, tip);
    set_accessible_label(pw->reload, tip);
}

static char *
address_display_url(const char *url)
{
    if (!url || !*url) return g_strdup("");
    if (!strchr(url, '%')) return g_strdup(url);
    char *dec = g_uri_unescape_string(url, NULL);
    if (!dec) return g_strdup(url);
    if (!g_utf8_validate(dec, -1, NULL)) {
        g_free(dec);
        return g_strdup(url);
    }
    for (const char *p = dec; *p; p = g_utf8_next_char(p)) {
        gunichar c = g_utf8_get_char(p);
        if (c < 0x20 || c == 0x7f ||
            (c >= 0x200e && c <= 0x200f) ||
            (c >= 0x202a && c <= 0x202e) ||
            (c >= 0x2066 && c <= 0x2069)) {
            g_free(dec);
            return g_strdup(url);
        }
    }
    return dec;
}

static void
set_address_text(ProcWindow *pw, const char *url)
{
    char *shown = address_display_url(url);
    gtk_editable_set_text(GTK_EDITABLE(pw->address), shown);
    g_free(shown);
}

static void
update_security_indicator(ProcWindow *pw, NsProcView *v)
{
    GtkEntry *entry = pw->address ? GTK_ENTRY(pw->address) : NULL;
    if (!entry)
        return;

    const char *url = v ? ns_proc_view_url(v) : NULL;
    int sec = v ? ns_proc_view_security(v) : NS_SEC_NONE;
    const char *icon_name = NULL, *label = NULL;
    switch (sec) {
    case NS_SEC_SECURE:
        icon_name = "southstar-lock-symbolic";
        label = ns_i18n("Secure — the certificate is valid");
        break;
    case NS_SEC_INVALID:
        icon_name = "southstar-warning-symbolic";
        label = ns_i18n("Not secure — the certificate is not trusted");
        break;
    case NS_SEC_PLAIN:
        icon_name = "southstar-lock-open-symbolic";
        label = ns_i18n("Not secure — the connection is not encrypted");
        break;
    default:
        break;
    }
    toggle_css_class(pw->address, "ns-insecure",
                     url && *url && sec == NS_SEC_INVALID);
    if (!icon_name || !url || !*url) {
        gboolean internal = url && g_str_has_prefix(url, "about:");
        const char *page_icon = internal ? "southstar"
                                         : "southstar-globe-symbolic";
        ns_icon_entry_set(entry, GTK_ENTRY_ICON_PRIMARY, page_icon);
        gtk_entry_set_icon_activatable(entry, GTK_ENTRY_ICON_PRIMARY, FALSE);
        gtk_entry_set_icon_tooltip_text(entry, GTK_ENTRY_ICON_PRIMARY,
                                        ns_i18n("Page location"));
        return;
    }
    ns_icon_entry_set(entry, GTK_ENTRY_ICON_PRIMARY, icon_name);
    gtk_entry_set_icon_activatable(entry, GTK_ENTRY_ICON_PRIMARY, FALSE);

    GString *tip = g_string_new(label);
    char *host = ns_url_host_from(url);
    if (host && *host)
        g_string_append_printf(tip, "\n%s", host);
    const char *ip = ns_proc_view_remote_ip(v);
    if (ip && *ip)
        g_string_append_printf(tip, "\n%s %s", ns_i18n("Server:"), ip);
    gtk_entry_set_icon_tooltip_text(entry, GTK_ENTRY_ICON_PRIMARY, tip->str);
    g_string_free(tip, TRUE);
    g_free(host);
}

static gboolean
desktop_prefers_dark(GtkWidget *styled)
{
    GdkRGBA fg;
    gtk_widget_get_color(styled, &fg);
    double luma = 0.2126 * fg.red + 0.7152 * fg.green + 0.0722 * fg.blue;
    return luma > 0.5;
}

static void
apply_color_scheme(ProcWindow *pw)
{
    const ns_config *cfg = ns_config_get();
    gboolean dark;
    switch (cfg ? cfg->color_scheme : NS_COLOR_SCHEME_PREF_AUTO) {
    case NS_COLOR_SCHEME_PREF_LIGHT: dark = FALSE; break;
    case NS_COLOR_SCHEME_PREF_DARK:  dark = TRUE;  break;
    default: dark = desktop_prefers_dark(pw->window); break;
    }
    const char *want = dark ? "dark" : "light";
    const char *have = g_getenv("NS_COLOR_SCHEME");
    if (have && g_str_equal(have, want))
        return;
    g_setenv("NS_COLOR_SCHEME", want, TRUE);
    ns_browser_set_color_scheme(dark);
    int pages = gtk_notebook_get_n_pages(GTK_NOTEBOOK(pw->notebook));
    for (int i = 0; i < pages; i++) {
        NsProcView *v = view_for_page(
            gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), i));
        if (!v) continue;
        ns_proc_view_set_color_scheme(v, dark);
        const char *url = ns_proc_view_url(v);
        if (url && *url)
            ns_proc_view_reload(v);
    }
}

static void
on_theme_changed(GObject *settings, GParamSpec *pspec, gpointer ud)
{
    (void)settings; (void)pspec;
    apply_color_scheme(ud);
}

static void
update_zoom_indicator(ProcWindow *pw, NsProcView *v)
{
    if (!pw->zoom_button)
        return;
    int percent = v ? ns_proc_view_zoom_percent(v) : 100;
    if (percent == 100) {
        gtk_widget_set_visible(pw->zoom_button, FALSE);
        return;
    }
    char label[16];
    g_snprintf(label, sizeof label, "%d%%", percent);
    gtk_button_set_label(GTK_BUTTON(pw->zoom_button), label);
    gtk_widget_set_visible(pw->zoom_button, TRUE);
}

static void
on_zoom_indicator_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_zoom_reset(v);
}

static gboolean
current_page_bookmarked(ProcWindow *pw)
{
    NsProcView *v = current_view(pw);
    const char *url = v ? ns_proc_view_url(v) : NULL;
    return url && *url && pw->bookmarks &&
           ns_bookmarks_contains(pw->bookmarks, url);
}

static void
update_bookmark_indicator(ProcWindow *pw)
{
    GtkEntry *entry = GTK_ENTRY(pw->address);
    NsProcView *v = current_view(pw);
    const char *url = v ? ns_proc_view_url(v) : NULL;
    gboolean saved = current_page_bookmarked(pw);
    toggle_css_class(pw->address, "ns-bookmarked", saved);
    if (!url || !*url) {
        gtk_entry_set_icon_from_icon_name(entry, GTK_ENTRY_ICON_SECONDARY,
                                          NULL);
        return;
    }
    const char *star = saved ? "southstar-star-filled-symbolic"
                             : "southstar-star-symbolic";
    ns_icon_entry_set(entry, GTK_ENTRY_ICON_SECONDARY, star);
    gtk_entry_set_icon_tooltip_text(entry, GTK_ENTRY_ICON_SECONDARY,
                                    saved ? ns_i18n("Remove this bookmark")
                                          : ns_i18n("Bookmark this page"));
}

static void
update_chrome(ProcWindow *pw)
{
    NsProcView *v = current_view(pw);
    if (!v) {
        gtk_editable_set_text(GTK_EDITABLE(pw->address), "");
        gtk_window_set_title(GTK_WINDOW(pw->window), ns_brand_versioned());
        gtk_widget_set_sensitive(pw->back, FALSE);
        gtk_widget_set_sensitive(pw->forward, FALSE);
        update_security_indicator(pw, NULL);
        set_loading_ui(pw, FALSE);
        return;
    }
    set_loading_ui(pw, ns_proc_view_is_loading(v));
    toggle_css_class(pw->toolbar, "ns-private", ns_proc_view_is_private(v));
    const char *url = ns_proc_view_url(v);
    const char *title = ns_proc_view_title(v);
    set_address_text(pw, url);
    const char *brand = ns_brand_versioned();
    char *wt = title && *title ? g_strdup_printf("%s — %s", title, brand)
                               : g_strdup(brand);
    gtk_window_set_title(GTK_WINDOW(pw->window), wt);
    g_free(wt);
    gtk_widget_set_sensitive(pw->back, ns_proc_view_can_back(v));
    gtk_widget_set_sensitive(pw->forward, ns_proc_view_can_forward(v));
    update_security_indicator(pw, v);
    update_zoom_indicator(pw, v);
    update_bookmark_indicator(pw);
}

static void proc_window_add_tab(ProcWindow *pw, const char *url,
                                gboolean foreground);
static void proc_window_add_tab_full(ProcWindow *pw, const char *url,
                                     gboolean foreground,
                                     gboolean private_mode);

static void
pw_render_status(ProcWindow *pw)
{
    const char *base = pw->status_base ? pw->status_base : "";
    if (pw->webgl_active) {
        char *composed = *base
            ? g_strconcat(base, " · ", ns_i18n("WebGL enabled"), NULL)
            : g_strdup(ns_i18n("WebGL enabled"));
        gtk_label_set_text(GTK_LABEL(pw->status), composed);
        g_free(composed);
    } else {
        gtk_label_set_text(GTK_LABEL(pw->status), base);
    }
    gtk_widget_set_visible(pw->status,
                           !pw->element_fullscreen &&
                           (*base != '\0' || pw->webgl_active));
}

static void
pw_set_status_persistent(ProcWindow *pw, const char *text)
{
    if (pw->status_timer) {
        g_source_remove(pw->status_timer);
        pw->status_timer = 0;
    }
    g_free(pw->status_base);
    pw->status_base = g_strdup(text ? text : "");
    pw_render_status(pw);
}

static gboolean
pw_status_expire(gpointer data)
{
    ProcWindow *pw = data;
    pw->status_timer = 0;
    g_clear_pointer(&pw->status_base, g_free);
    pw_render_status(pw);
    return G_SOURCE_REMOVE;
}

static void
pw_set_status(ProcWindow *pw, const char *text)
{
    if (pw->status_timer)
        g_source_remove(pw->status_timer);
    g_free(pw->status_base);
    pw->status_base = g_strdup(text ? text : "");
    pw_render_status(pw);
    pw->status_timer = *pw->status_base
        ? g_timeout_add_seconds(5, pw_status_expire, pw) : 0;
}

static gboolean
pw_fullscreen_notice_expire(gpointer data)
{
    ProcWindow *pw = data;
    pw->fullscreen_notice_timer = 0;
    gtk_widget_set_visible(pw->fullscreen_notice, FALSE);
    return G_SOURCE_REMOVE;
}

static void
pw_hide_fullscreen_notice(ProcWindow *pw)
{
    if (pw->fullscreen_notice_timer) {
        g_source_remove(pw->fullscreen_notice_timer);
        pw->fullscreen_notice_timer = 0;
    }
    gtk_widget_set_visible(pw->fullscreen_notice, FALSE);
}

static void
pw_show_fullscreen_notice(ProcWindow *pw, NsProcView *v)
{
    const char *url = v ? ns_proc_view_url(v) : NULL;
    char *host = url ? ns_url_host_from(url) : NULL;
    const char *site = host && *host ? host : ns_i18n("This page");
    char *site_markup = g_markup_escape_text(site, -1);
    char *hint = g_markup_escape_text(
        ns_i18n("is now full screen — press Esc to exit"), -1);
    char *markup = g_strdup_printf("<b>%s</b> %s", site_markup, hint);
    gtk_label_set_markup(GTK_LABEL(pw->fullscreen_notice), markup);
    g_free(markup);
    g_free(hint);
    g_free(site_markup);
    g_free(host);
    gtk_widget_set_visible(pw->fullscreen_notice, TRUE);
    if (pw->fullscreen_notice_timer)
        g_source_remove(pw->fullscreen_notice_timer);
    pw->fullscreen_notice_timer =
        g_timeout_add_seconds(NS_FULLSCREEN_NOTICE_SECONDS,
                              pw_fullscreen_notice_expire, pw);
}

static void
pw_set_element_fullscreen(ProcWindow *pw, gboolean active)
{
    if (!pw || pw->element_fullscreen == active) return;
    pw->element_fullscreen = active;
    pw->fullscreen_view = active ? current_view(pw) : NULL;
    gtk_widget_set_visible(pw->header, !active);
    gtk_widget_set_visible(pw->toolbar, !active);
    pw_render_status(pw);
    if (active) {
        gtk_window_fullscreen(GTK_WINDOW(pw->window));
        pw_show_fullscreen_notice(pw, pw->fullscreen_view);
    } else {
        pw_hide_fullscreen_notice(pw);
        gtk_window_unfullscreen(GTK_WINDOW(pw->window));
    }
}

static void
pw_leave_element_fullscreen(ProcWindow *pw)
{
    if (!pw->element_fullscreen) return;
    NsProcView *v = pw->fullscreen_view;
    pw_set_element_fullscreen(pw, FALSE);
    if (v) ns_proc_view_exit_fullscreen(v);
}

static void
set_tab_loading(GtkWidget *page, gboolean loading)
{
    GtkWidget *icon = g_object_get_data(G_OBJECT(page), "ns-tab-icon");
    GtkWidget *spinner = g_object_get_data(G_OBJECT(page), "ns-tab-spinner");
    if (!icon || !spinner)
        return;
    gtk_spinner_set_spinning(GTK_SPINNER(spinner), loading);
    gtk_widget_set_visible(spinner, loading);
    gtk_widget_set_visible(icon, !loading);
}

static void
on_view_notify(NsProcView *v, NsProcEvent evt, const char *text,
               gpointer user_data)
{
    ProcWindow *pw = user_data;
    GtkWidget *page = ns_proc_view_widget(v);
    int idx = page ? gtk_notebook_page_num(GTK_NOTEBOOK(pw->notebook), page)
                   : -1;
    gboolean is_current = (v == current_view(pw));

    switch (evt) {
    case NS_PROC_EVT_TITLE: {
        if (!ns_proc_view_is_private(v))
            ns_history_record(ns_proc_view_url(v), text);
        if (idx >= 0) {
            GtkWidget *p =
                gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), idx);
            GtkWidget *label = g_object_get_data(G_OBJECT(p), "ns-tab-label");
            const char *t = text && *text ? text : ns_i18n("Untitled");
            char *clip = g_strndup(t, 40);
            if (label)
                gtk_label_set_text(GTK_LABEL(label), clip);
            g_free(clip);
        }
        if (is_current)
            update_chrome(pw);
        break;
    }
    case NS_PROC_EVT_URL:
        if (pw->element_fullscreen && v == pw->fullscreen_view)
            pw_set_element_fullscreen(pw, FALSE);
        if (is_current) {
            set_address_text(pw, text);
            pw->webgl_active = FALSE;
            pw_set_status_persistent(pw, NULL);
        }
        break;
    case NS_PROC_EVT_STATUS:
        if (is_current)
            pw_set_status_persistent(pw, text);
        break;
    case NS_PROC_EVT_WEBGL:
        if (is_current && !pw->webgl_active) {
            pw->webgl_active = TRUE;
            pw_render_status(pw);
        }
        break;
    case NS_PROC_EVT_FULLSCREEN:
        if (is_current)
            pw_set_element_fullscreen(
                pw, text && strcmp(text, "fullscreen-enter") == 0);
        break;
    case NS_PROC_EVT_HISTORY:
        if (is_current) {
            gtk_widget_set_sensitive(pw->back, ns_proc_view_can_back(v));
            gtk_widget_set_sensitive(pw->forward, ns_proc_view_can_forward(v));
        }
        break;
    case NS_PROC_EVT_NEWTAB:
        if (text && *text)
            proc_window_add_tab_full(pw, text, FALSE,
                                     ns_proc_view_is_private(v));
        break;
    case NS_PROC_EVT_LOADING: {
        gboolean loading = text && *text == '1';
        if (idx >= 0)
            set_tab_loading(
                gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), idx),
                loading);
        if (is_current) {
            gboolean was_loading = pw->loading;
            set_loading_ui(pw, loading);
            if (was_loading && !loading)
                pw_set_status(pw, ns_i18n("Done"));
        }
        break;
    }
    case NS_PROC_EVT_ZOOM:
        if (is_current)
            update_zoom_indicator(pw, v);
        break;
    case NS_PROC_EVT_DOWNLOAD:
        if (text && *text) {
            char **parts = g_strsplit(text, "\t", 2);
            const char *page_url = ns_proc_view_url(v);
            gboolean local_target =
                g_ascii_strncasecmp(parts[0], "file:", 5) == 0;
            gboolean local_page =
                page_url && g_ascii_strncasecmp(page_url, "file:", 5) == 0;
            if (!local_target || local_page)
                ns_downloads_start(pw->window, parts[0],
                                   parts[1] && *parts[1] ? parts[1] : NULL);
            g_strfreev(parts);
        }
        break;
    case NS_PROC_EVT_FAVICON:
        if (idx >= 0 && !ns_proc_view_is_private(v)) {
            GtkWidget *p =
                gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), idx);
            GtkWidget *icon = g_object_get_data(G_OBJECT(p), "ns-tab-icon");
            GdkPaintable *fav = ns_proc_view_favicon(v);
            if (icon && fav)
                gtk_image_set_from_paintable(GTK_IMAGE(icon), fav);
            else if (icon)
                ns_icon_image_set(GTK_IMAGE(icon),
                                  "southstar-page-symbolic");
            if (icon)
                gtk_image_set_pixel_size(GTK_IMAGE(icon), 16);
        }
        break;
    }
}

static void
update_active_tab(ProcWindow *pw)
{
    GtkWidget *current = NULL;
    int idx = gtk_notebook_get_current_page(GTK_NOTEBOOK(pw->notebook));
    if (idx >= 0)
        current = gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), idx);
    gboolean closable =
        gtk_notebook_get_n_pages(GTK_NOTEBOOK(pw->notebook)) > 1;
    for (GtkWidget *w = gtk_widget_get_first_child(pw->tabstrip);
         w; w = gtk_widget_get_next_sibling(w)) {
        GtkWidget *page = g_object_get_data(G_OBJECT(w), "ns-page");
        if (!page)
            continue;
        toggle_css_class(w, "ns-tab-active", page == current);
        GtkWidget *close = g_object_get_data(G_OBJECT(w), "ns-tab-close");
        if (close)
            gtk_widget_set_visible(close, closable);
    }
}

static void
on_tab_clicked(GtkButton *button, gpointer user_data)
{
    ProcWindow *pw = g_object_get_data(G_OBJECT(button), "ns-pw");
    GtkWidget *page = user_data;
    int idx = gtk_notebook_page_num(GTK_NOTEBOOK(pw->notebook), page);
    if (idx >= 0)
        gtk_notebook_set_current_page(GTK_NOTEBOOK(pw->notebook), idx);
}

static void
proc_window_close_page(ProcWindow *pw, GtkWidget *page)
{
    GtkWidget *wrapper = g_object_get_data(G_OBJECT(page), "ns-strip-tab");
    int idx = gtk_notebook_page_num(GTK_NOTEBOOK(pw->notebook), page);
    if (pw->element_fullscreen && view_for_page(page) == pw->fullscreen_view)
        pw_set_element_fullscreen(pw, FALSE);
    if (idx >= 0)
        gtk_notebook_remove_page(GTK_NOTEBOOK(pw->notebook), idx);
    if (wrapper)
        gtk_box_remove(GTK_BOX(pw->tabstrip), wrapper);
    if (gtk_notebook_get_n_pages(GTK_NOTEBOOK(pw->notebook)) == 0)
        gtk_window_close(GTK_WINDOW(pw->window));
    else
        update_active_tab(pw);
}

static void
on_tab_close(GtkButton *button, gpointer user_data)
{
    ProcWindow *pw = g_object_get_data(G_OBJECT(button), "ns-pw");
    proc_window_close_page(pw, user_data);
}

static void
proc_window_add_tab_full(ProcWindow *pw, const char *url, gboolean foreground,
                         gboolean private_mode)
{
    if (!private_mode && ns_rproc_single_process_enabled() &&
        gtk_notebook_get_n_pages(GTK_NOTEBOOK(pw->notebook)) > 0) {
        NsProcView *cur = current_view(pw);
        if (cur) {
            char *r = normalize_url(url);
            gtk_editable_set_text(GTK_EDITABLE(pw->address), r);
            ns_proc_view_load(cur, r);
            g_free(r);
            return;
        }
    }

    NsProcView *v = ns_proc_view_new();
    if (private_mode)
        ns_proc_view_set_private(v, TRUE);
    ns_proc_view_set_notify(v, on_view_notify, pw);
    GtkWidget *page = ns_proc_view_widget(v);
    g_object_set_data(G_OBJECT(page), "ns-proc-view", v);

    GtkWidget *wrapper = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 0);
    gtk_widget_add_css_class(wrapper, "ns-tab");
    if (private_mode)
        gtk_widget_add_css_class(wrapper, "ns-tab-private");

    GtkWidget *tabbtn = gtk_button_new();
    gtk_button_set_has_frame(GTK_BUTTON(tabbtn), FALSE);
    gtk_widget_add_css_class(tabbtn, "ns-tab-label");
    GtkWidget *tabcontent = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 5);
    GtkWidget *icon = ns_icon_image_new(
        private_mode ? "southstar-private-symbolic"
                     : "southstar-page-symbolic");
    gtk_image_set_pixel_size(GTK_IMAGE(icon), 16);
    GtkWidget *spinner = gtk_spinner_new();
    gtk_widget_set_size_request(spinner, 16, 16);
    gtk_widget_set_visible(spinner, FALSE);
    if (private_mode)
        gtk_widget_set_tooltip_text(tabbtn, ns_i18n("Private tab"));
    GtkWidget *label = gtk_label_new(
        private_mode ? ns_i18n("Private tab") : ns_i18n("New Tab"));
    gtk_label_set_ellipsize(GTK_LABEL(label), PANGO_ELLIPSIZE_END);
    gtk_label_set_width_chars(GTK_LABEL(label), 10);
    gtk_label_set_max_width_chars(GTK_LABEL(label), 22);
    gtk_label_set_xalign(GTK_LABEL(label), 0.0);
    gtk_box_append(GTK_BOX(tabcontent), spinner);
    gtk_box_append(GTK_BOX(tabcontent), icon);
    gtk_box_append(GTK_BOX(tabcontent), label);
    gtk_button_set_child(GTK_BUTTON(tabbtn), tabcontent);
    g_object_set_data(G_OBJECT(tabbtn), "ns-pw", pw);
    g_signal_connect(tabbtn, "clicked", G_CALLBACK(on_tab_clicked), page);
    gtk_box_append(GTK_BOX(wrapper), tabbtn);

    GtkWidget *close = ns_icon_button_new("southstar-close-symbolic");
    gtk_button_set_has_frame(GTK_BUTTON(close), FALSE);
    gtk_widget_add_css_class(close, "ns-tab-close");
    gtk_widget_set_valign(close, GTK_ALIGN_CENTER);
    gtk_widget_set_tooltip_text(close, ns_i18n("Close tab"));
    set_accessible_label(close, ns_i18n("Close tab"));
    g_object_set_data(G_OBJECT(close), "ns-pw", pw);
    g_signal_connect(close, "clicked", G_CALLBACK(on_tab_close), page);
    gtk_box_append(GTK_BOX(wrapper), close);

    g_object_set_data(G_OBJECT(wrapper), "ns-page", page);
    g_object_set_data(G_OBJECT(wrapper), "ns-tab-close", close);
    g_object_set_data(G_OBJECT(page), "ns-tab-label", label);
    g_object_set_data(G_OBJECT(page), "ns-tab-icon", icon);
    g_object_set_data(G_OBJECT(page), "ns-tab-spinner", spinner);
    g_object_set_data(G_OBJECT(page), "ns-strip-tab", wrapper);

    GtkWidget *blank = gtk_label_new(NULL);
    int idx = gtk_notebook_append_page(GTK_NOTEBOOK(pw->notebook), page, blank);

    g_object_ref(pw->newtab_btn);
    gtk_box_remove(GTK_BOX(pw->tabstrip), pw->newtab_btn);
    gtk_box_append(GTK_BOX(pw->tabstrip), wrapper);
    gtk_box_append(GTK_BOX(pw->tabstrip), pw->newtab_btn);
    g_object_unref(pw->newtab_btn);

    if (foreground)
        gtk_notebook_set_current_page(GTK_NOTEBOOK(pw->notebook), idx);
    update_active_tab(pw);

    char *resolved = normalize_url(url);
    ns_proc_view_load(v, resolved);
    g_free(resolved);
}

static void
proc_window_add_tab(ProcWindow *pw, const char *url, gboolean foreground)
{
    proc_window_add_tab_full(pw, url, foreground, FALSE);
}

static gboolean
address_select_all_idle(gpointer user_data)
{
    ProcWindow *pw = user_data;
    if (pw->address && GTK_IS_EDITABLE(pw->address))
        gtk_editable_select_region(GTK_EDITABLE(pw->address), 0, -1);
    return G_SOURCE_REMOVE;
}

static void
on_address_focus_enter(GtkEventControllerFocus *ctrl, gpointer user_data)
{
    (void)ctrl;
    g_idle_add(address_select_all_idle, user_data);
}

static gboolean
address_click_select_all_idle(gpointer user_data)
{
    ProcWindow *pw = user_data;
    if (pw->address && GTK_IS_EDITABLE(pw->address) &&
        !gtk_editable_get_selection_bounds(GTK_EDITABLE(pw->address), NULL,
                                           NULL))
        gtk_editable_select_region(GTK_EDITABLE(pw->address), 0, -1);
    return G_SOURCE_REMOVE;
}

static void
on_address_click_pressed(GtkGestureClick *gesture, int n_press, double x,
                         double y, gpointer user_data)
{
    (void)gesture;
    (void)x;
    (void)y;
    ProcWindow *pw = user_data;
    pw->address_click_focuses =
        n_press == 1 && !(gtk_widget_get_state_flags(pw->address) &
                          GTK_STATE_FLAG_FOCUS_WITHIN);
}

static void
on_address_click_released(GtkGestureClick *gesture, int n_press, double x,
                          double y, gpointer user_data)
{
    (void)gesture;
    (void)n_press;
    (void)x;
    (void)y;
    ProcWindow *pw = user_data;
    if (!pw->address_click_focuses)
        return;
    pw->address_click_focuses = FALSE;
    g_idle_add(address_click_select_all_idle, pw);
}

static void
on_address_activate(GtkEntry *entry, gpointer user_data)
{
    ProcWindow *pw = user_data;
    NsProcView *v = current_view(pw);
    char *resolved = normalize_url(gtk_editable_get_text(GTK_EDITABLE(entry)));
    if (!*resolved) {
        g_free(resolved);
        return;
    }
    if (!v) {
        proc_window_add_tab(pw, resolved, TRUE);
        v = current_view(pw);
    } else {
        gtk_editable_set_text(GTK_EDITABLE(pw->address), resolved);
        ns_proc_view_load(v, resolved);
    }
    if (v)
        ns_proc_view_focus(v);
    g_free(resolved);
}

static gboolean
on_address_key_pressed(GtkEventControllerKey *controller, guint keyval,
                       guint keycode, GdkModifierType state, gpointer ud)
{
    (void)controller;
    (void)keycode;
    ProcWindow *pw = ud;
    if (keyval != GDK_KEY_Return && keyval != GDK_KEY_KP_Enter)
        return FALSE;
    gboolean ctrl  = (state & GDK_CONTROL_MASK) != 0;
    gboolean shift = (state & GDK_SHIFT_MASK) != 0;
    if (!ctrl && !shift)
        return FALSE;
    char *text =
        g_strstrip(g_strdup(gtk_editable_get_text(GTK_EDITABLE(pw->address))));
    if (!*text || strstr(text, "://") || strchr(text, ' ') ||
        g_str_has_prefix(text, "about:") ||
        g_str_has_prefix(text, "file:") ||
        g_str_has_prefix(text, "data:") ||
        g_str_has_prefix(text, "view-source:")) {
        g_free(text);
        return FALSE;
    }
    const char *suffix = ctrl && shift ? ".org" : shift ? ".net" : ".com";
    const char *prefix = g_str_has_prefix(text, "www.") ? "" : "www.";
    char *host = g_str_has_suffix(text, suffix)
        ? g_strconcat(prefix, text, NULL)
        : g_strconcat(prefix, text, suffix, NULL);
    g_free(text);
    gtk_editable_set_text(GTK_EDITABLE(pw->address), host);
    g_free(host);
    on_address_activate(GTK_ENTRY(pw->address), pw);
    return TRUE;
}

static void
on_back_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_back(v);
}

static void
on_forward_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_forward(v);
}

static void
on_reload_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    ProcWindow *pw = ud;
    NsProcView *v = current_view(pw);
    if (!v)
        return;
    if (pw->loading)
        ns_proc_view_stop(v);
    else
        ns_proc_view_reload(v);
}

static void
load_home_page(NsProcView *v)
{
    ns_config_lock();
    ns_config_reload();
    const ns_config *cfg = ns_config_get();
    char *home = normalize_url(cfg && cfg->home_url && *cfg->home_url
                               ? cfg->home_url : "about:start");
    ns_config_unlock();
    ns_proc_view_load(v, home);
    g_free(home);
}

static void
on_home_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    NsProcView *v = current_view(ud);
    if (v)
        load_home_page(v);
}

static void
on_logo_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_load(v, "https://github.com/nordstjernen-web/southstar-browser");
}

static void
on_address_icon_press(GtkEntry *entry, GtkEntryIconPosition pos, gpointer ud)
{
    (void)entry;
    ProcWindow *pw = ud;
    if (pos == GTK_ENTRY_ICON_SECONDARY)
        g_action_group_activate_action(G_ACTION_GROUP(pw->window),
                                       "bookmark-page", NULL);
}

static void
on_downloads_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    ns_downloads_show(((ProcWindow *)ud)->window);
}

static void
on_newtab_clicked(GtkButton *b, gpointer ud)
{
    (void)b;
    proc_window_add_tab(ud, "about:start", TRUE);
}

static void
on_switch_page(GtkNotebook *nb, GtkWidget *page, guint num, gpointer ud)
{
    (void)nb;
    (void)num;
    ProcWindow *pw = ud;
    if (pw->element_fullscreen && view_for_page(page) != pw->fullscreen_view)
        pw_leave_element_fullscreen(pw);
    update_active_tab(pw);
    update_chrome(pw);
}

static void
act_back(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_back(v);
}

static void
act_forward(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_forward(v);
}

static void
act_reload(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_reload(v);
}

static void
act_hard_reload(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (!v) return;
    ns_cache_clear();
    ns_proc_view_reload(v);
}

static void
act_find(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_find_open(v);
}

static void
act_console(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_toggle_console(v);
}

static void
act_bookmark_page(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    ProcWindow *pw = ud;
    NsProcView *v = current_view(pw);
    if (!v || !pw->bookmarks)
        return;
    const char *url = ns_proc_view_url(v);
    if (!url || !*url)
        return;
    if (ns_bookmarks_contains(pw->bookmarks, url)) {
        ns_bookmarks_remove(pw->bookmarks, url);
        pw_set_status(pw, ns_i18n("Bookmark removed"));
    } else {
        ns_bookmarks_add(pw->bookmarks, url, ns_proc_view_title(v));
        pw_set_status(pw, ns_i18n("Bookmark added"));
    }
    update_bookmark_indicator(pw);
}

static void
act_view_source(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (!v) return;
    const char *url = ns_proc_view_url(v);
    if (!url || !*url || g_str_has_prefix(url, "view-source:"))
        return;
    char *src_url = g_strconcat("view-source:", url, NULL);
    ns_proc_view_load(v, src_url);
    g_free(src_url);
}

static void
act_home(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        load_home_page(v);
}

static void
act_new_tab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    proc_window_add_tab(ud, "about:start", TRUE);
}

static void
act_new_private_tab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    proc_window_add_tab_full(ud, "about:start", TRUE, TRUE);
}

static void
act_close_tab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    ProcWindow *pw = ud;
    int idx = gtk_notebook_get_current_page(GTK_NOTEBOOK(pw->notebook));
    if (idx < 0)
        return;
    GtkWidget *page = gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), idx);
    proc_window_close_page(pw, page);
}

static void
act_focus_address(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    ProcWindow *pw = ud;
    gtk_widget_grab_focus(pw->address);
    gtk_editable_select_region(GTK_EDITABLE(pw->address), 0, -1);
}

static void
act_focus_page(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    ProcWindow *pw = ud;
    NsProcView *v = current_view(pw);
    GtkWidget *focus = gtk_window_get_focus(GTK_WINDOW(pw->window));
    gboolean editing_address = focus && pw->address &&
        (focus == pw->address || gtk_widget_is_ancestor(focus, pw->address));
    if (editing_address)
        set_address_text(pw, v ? ns_proc_view_url(v) : "");
    else if (ns_proc_view_find_close(v))
        return;
    else if (v && ns_proc_view_is_loading(v))
        ns_proc_view_stop(v);
    if (v)
        ns_proc_view_focus(v);
}

static void
act_zoom_in(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_zoom_in(v);
}

static void
act_zoom_out(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_zoom_out(v);
}

static void
act_zoom_reset(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    NsProcView *v = current_view(ud);
    if (v)
        ns_proc_view_zoom_reset(v);
}

static void
act_step_tab(ProcWindow *pw, int delta)
{
    GtkNotebook *nb = GTK_NOTEBOOK(pw->notebook);
    int n = gtk_notebook_get_n_pages(nb);
    if (n <= 1)
        return;
    int idx = gtk_notebook_get_current_page(nb);
    idx = ((idx + delta) % n + n) % n;
    gtk_notebook_set_current_page(nb, idx);
}

static void
act_next_tab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    act_step_tab(ud, 1);
}

static void
act_prev_tab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    act_step_tab(ud, -1);
}

static void
act_quit(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a;
    (void)p;
    ProcWindow *pw = ud;
    gtk_window_close(GTK_WINDOW(pw->window));
}

static void act_about(GSimpleAction *action, GVariant *parameter,
                      gpointer user_data);
static void act_settings(GSimpleAction *action, GVariant *parameter,
                         gpointer user_data);
static void act_history(GSimpleAction *action, GVariant *parameter,
                        gpointer user_data);
static void act_print(GSimpleAction *action, GVariant *parameter,
                      gpointer user_data);
static void act_save_pdf(GSimpleAction *action, GVariant *parameter,
                         gpointer user_data);
static void act_save_image(GSimpleAction *action, GVariant *parameter,
                           gpointer user_data);
static void act_fullscreen(GSimpleAction *action, GVariant *parameter,
                           gpointer user_data);
static void on_bookmarks_clicked(GtkButton *button, gpointer user_data);

static void
act_downloads(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ns_downloads_show(((ProcWindow *)user_data)->window);
}

static void
act_task_manager(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ProcWindow *pw = user_data;
    ns_task_manager_show(pw->window, pw->notebook);
}

static void
install_action(ProcWindow *pw, const char *name, GCallback cb,
               const char *const *accels)
{
    GSimpleAction *act = g_simple_action_new(name, NULL);
    g_signal_connect(act, "activate", cb, pw);
    g_action_map_add_action(G_ACTION_MAP(pw->window), G_ACTION(act));
    g_object_unref(act);
    if (accels) {
        char *full = g_strconcat("win.", name, NULL);
        gtk_application_set_accels_for_action(pw->app, full, accels);
        g_free(full);
    }
}

static void
install_shortcuts(ProcWindow *pw)
{
    install_action(pw, "back", G_CALLBACK(act_back),
                   (const char *[]){ "<Alt>Left", NULL });
    install_action(pw, "forward", G_CALLBACK(act_forward),
                   (const char *[]){ "<Alt>Right", NULL });
    install_action(pw, "reload", G_CALLBACK(act_reload),
                   (const char *[]){ "<Ctrl>r", "F5", NULL });
    install_action(pw, "hard-reload", G_CALLBACK(act_hard_reload),
                   (const char *[]){ "<Ctrl><Shift>r", "<Ctrl>F5", NULL });
    install_action(pw, "find", G_CALLBACK(act_find),
                   (const char *[]){ "<Ctrl>f", NULL });
    install_action(pw, "console", G_CALLBACK(act_console),
                   (const char *[]){ "<Ctrl><Shift>j", "F12", NULL });
    install_action(pw, "view-source", G_CALLBACK(act_view_source),
                   (const char *[]){ "<Ctrl>u", NULL });
    install_action(pw, "bookmark-page", G_CALLBACK(act_bookmark_page),
                   (const char *[]){ "<Ctrl>d", NULL });
    install_action(pw, "home", G_CALLBACK(act_home),
                   (const char *[]){ "<Alt>Home", NULL });
    install_action(pw, "new-tab", G_CALLBACK(act_new_tab),
                   (const char *[]){ "<Ctrl>t", NULL });
    install_action(pw, "new-private-tab", G_CALLBACK(act_new_private_tab),
                   (const char *[]){ "<Ctrl><Alt>p", NULL });
    install_action(pw, "close-tab", G_CALLBACK(act_close_tab),
                   (const char *[]){ "<Ctrl>w", NULL });
    install_action(pw, "focus-address", G_CALLBACK(act_focus_address),
                   (const char *[]){ "<Ctrl>l", NULL });
    install_action(pw, "focus-page", G_CALLBACK(act_focus_page),
                   (const char *[]){ "Escape", NULL });
    install_action(pw, "zoom-in", G_CALLBACK(act_zoom_in),
                   (const char *[]){ "<Ctrl>plus", "<Ctrl>equal",
                                     "<Ctrl>KP_Add", NULL });
    install_action(pw, "zoom-out", G_CALLBACK(act_zoom_out),
                   (const char *[]){ "<Ctrl>minus", "<Ctrl>KP_Subtract",
                                     NULL });
    install_action(pw, "zoom-reset", G_CALLBACK(act_zoom_reset),
                   (const char *[]){ "<Ctrl>0", "<Ctrl>KP_0", NULL });
    install_action(pw, "next-tab", G_CALLBACK(act_next_tab),
                   (const char *[]){ "<Ctrl>Page_Down", "<Ctrl>Tab", NULL });
    install_action(pw, "prev-tab", G_CALLBACK(act_prev_tab),
                   (const char *[]){ "<Ctrl>Page_Up", "<Ctrl><Shift>Tab",
                                     NULL });
    install_action(pw, "task-manager", G_CALLBACK(act_task_manager),
                   (const char *[]){ "<Shift>Escape", NULL });
    install_action(pw, "downloads", G_CALLBACK(act_downloads),
                   (const char *[]){ "<Ctrl>j", NULL });
    install_action(pw, "about", G_CALLBACK(act_about), NULL);
    install_action(pw, "history", G_CALLBACK(act_history),
                   (const char *[]){ "<Ctrl>h", NULL });
    install_action(pw, "print", G_CALLBACK(act_print),
                   (const char *[]){ "<Ctrl>p", NULL });
    install_action(pw, "save-pdf", G_CALLBACK(act_save_pdf), NULL);
    install_action(pw, "save-image", G_CALLBACK(act_save_image), NULL);
    install_action(pw, "fullscreen", G_CALLBACK(act_fullscreen), NULL);
    install_action(pw, "settings", G_CALLBACK(act_settings),
                   (const char *[]){ "<Ctrl>comma", NULL });
    install_action(pw, "quit", G_CALLBACK(act_quit),
                   (const char *[]){ "<Ctrl>q", NULL });

    if (ns_rproc_single_process_enabled()) {
        GAction *nt = g_action_map_lookup_action(G_ACTION_MAP(pw->window),
                                                 "new-tab");
        if (nt) g_simple_action_set_enabled(G_SIMPLE_ACTION(nt), FALSE);
        GAction *npt = g_action_map_lookup_action(G_ACTION_MAP(pw->window),
                                                  "new-private-tab");
        if (npt) g_simple_action_set_enabled(G_SIMPLE_ACTION(npt), FALSE);
    }
}

static gboolean
on_window_key_pressed(GtkEventControllerKey *controller, guint keyval,
                      guint keycode, GdkModifierType state, gpointer user_data)
{
    (void)controller;
    (void)keycode;
    (void)state;
    ProcWindow *pw = user_data;
    GtkWindow *win = GTK_WINDOW(pw->window);
    if (pw->element_fullscreen &&
        (keyval == GDK_KEY_F11 || keyval == GDK_KEY_Escape)) {
        pw_leave_element_fullscreen(pw);
        return TRUE;
    }
    if (keyval == GDK_KEY_F11) {
        if (gtk_window_is_fullscreen(win)) {
            gtk_window_unfullscreen(win);
            gtk_window_maximize(win);
        } else {
            gtk_window_fullscreen(win);
        }
        return TRUE;
    }
    if (keyval == GDK_KEY_Escape && gtk_window_is_fullscreen(win)) {
        gtk_window_unfullscreen(win);
        gtk_window_maximize(win);
        return TRUE;
    }
    return FALSE;
}

static void
menu_append_accel(GMenu *menu, const char *label, const char *action,
                  const char *accel)
{
    GMenuItem *item = g_menu_item_new(label, action);
    if (accel)
        g_menu_item_set_attribute(item, "accel", "s", accel);
    g_menu_append_item(menu, item);
    g_object_unref(item);
}

static ProcWindow *
proc_window_new(GtkApplication *app)
{
    ProcWindow *pw = g_new0(ProcWindow, 1);
    pw->app = app;
    pw->bookmarks = ns_bookmarks_load();
    pw->window = gtk_application_window_new(app);
    g_object_set_data_full(G_OBJECT(pw->window), "ns-procwindow", pw,
                           (GDestroyNotify)procwindow_free);
    gtk_window_set_title(GTK_WINDOW(pw->window), ns_brand_versioned());
    if (g_initial_win_w > 0 && g_initial_win_h > 0) {
        gtk_window_set_default_size(GTK_WINDOW(pw->window),
                                    g_initial_win_w, g_initial_win_h);
    } else {
        gtk_window_set_default_size(GTK_WINDOW(pw->window), 1024, 768);
        gtk_window_maximize(GTK_WINDOW(pw->window));
    }

    GtkEventController *winkeys = gtk_event_controller_key_new();
    gtk_event_controller_set_propagation_phase(winkeys, GTK_PHASE_CAPTURE);
    g_signal_connect(winkeys, "key-pressed",
                     G_CALLBACK(on_window_key_pressed), pw);
    gtk_widget_add_controller(pw->window, winkeys);

    pw->header = gtk_header_bar_new();
    gtk_header_bar_set_show_title_buttons(GTK_HEADER_BAR(pw->header), FALSE);
    pw->tabstrip = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);
    gtk_widget_add_css_class(pw->tabstrip, "ns-tabstrip");
    pw->newtab_btn = ns_icon_button_new("southstar-new-tab-symbolic");
    gtk_button_set_has_frame(GTK_BUTTON(pw->newtab_btn), FALSE);
    gtk_widget_add_css_class(pw->newtab_btn, "ns-newtab");
    gtk_widget_set_tooltip_text(pw->newtab_btn, ns_i18n("New tab"));
    set_accessible_label(pw->newtab_btn, ns_i18n("New tab"));
    g_signal_connect(pw->newtab_btn, "clicked",
                     G_CALLBACK(on_newtab_clicked), pw);
    gtk_widget_set_valign(pw->newtab_btn, GTK_ALIGN_CENTER);
    gtk_box_append(GTK_BOX(pw->tabstrip), pw->newtab_btn);
    if (ns_rproc_single_process_enabled())
        gtk_widget_set_visible(pw->newtab_btn, FALSE);
    gtk_header_bar_set_title_widget(GTK_HEADER_BAR(pw->header),
                                    ns_title_row_new(pw->tabstrip));
    gtk_window_set_titlebar(GTK_WINDOW(pw->window), pw->header);

    GtkWidget *vbox = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

    pw->toolbar = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 2);
    gtk_widget_add_css_class(pw->toolbar, "ns-toolbar");

    pw->back = toolbar_button("southstar-back-symbolic",
                              ns_i18n("Go back one page"),
                              G_CALLBACK(on_back_clicked), pw);
    pw->forward = toolbar_button("southstar-forward-symbolic",
                                 ns_i18n("Go forward one page"),
                                 G_CALLBACK(on_forward_clicked), pw);
    pw->reload = toolbar_button("southstar-reload-symbolic",
                                ns_i18n("Reload this page"),
                                G_CALLBACK(on_reload_clicked), pw);
    GtkWidget *home = toolbar_button("southstar-home-symbolic",
                                     ns_i18n("Go to the home page"),
                                     G_CALLBACK(on_home_clicked), pw);
    GtkWidget *downloads = toolbar_button("southstar-downloads-symbolic",
                                          ns_i18n("Show downloads"),
                                          G_CALLBACK(on_downloads_clicked), pw);

    pw->address = gtk_entry_new();
    gtk_widget_set_hexpand(pw->address, TRUE);
    gtk_widget_set_valign(pw->address, GTK_ALIGN_CENTER);
    gtk_widget_add_css_class(pw->address, "ns-address");
    ns_icon_entry_set(GTK_ENTRY(pw->address), GTK_ENTRY_ICON_PRIMARY,
                      "southstar-globe-symbolic");
    gtk_entry_set_icon_tooltip_text(GTK_ENTRY(pw->address),
                                    GTK_ENTRY_ICON_PRIMARY,
                                    ns_i18n("Page location"));
    gtk_entry_set_placeholder_text(GTK_ENTRY(pw->address),
                                   ns_i18n("Search or enter a URL"));
    set_accessible_label(pw->address, ns_i18n("Address and search bar"));
    g_signal_connect(pw->address, "activate",
                     G_CALLBACK(on_address_activate), pw);
    g_signal_connect(pw->address, "icon-press",
                     G_CALLBACK(on_address_icon_press), pw);
    GtkEventController *addr_focus = gtk_event_controller_focus_new();
    g_signal_connect(addr_focus, "enter",
                     G_CALLBACK(on_address_focus_enter), pw);
    gtk_widget_add_controller(pw->address, addr_focus);
    GtkGesture *addr_click = gtk_gesture_click_new();
    gtk_event_controller_set_propagation_phase(GTK_EVENT_CONTROLLER(addr_click),
                                               GTK_PHASE_CAPTURE);
    g_signal_connect(addr_click, "pressed",
                     G_CALLBACK(on_address_click_pressed), pw);
    g_signal_connect(addr_click, "released",
                     G_CALLBACK(on_address_click_released), pw);
    gtk_widget_add_controller(pw->address, GTK_EVENT_CONTROLLER(addr_click));
    GtkEventController *addr_keys = gtk_event_controller_key_new();
    gtk_event_controller_set_propagation_phase(addr_keys, GTK_PHASE_CAPTURE);
    g_signal_connect(addr_keys, "key-pressed",
                     G_CALLBACK(on_address_key_pressed), pw);
    gtk_widget_add_controller(pw->address, addr_keys);

    pw->zoom_button = gtk_button_new_with_label("100%");
    gtk_button_set_has_frame(GTK_BUTTON(pw->zoom_button), FALSE);
    gtk_widget_add_css_class(pw->zoom_button, "ns-zoom");
    gtk_widget_set_valign(pw->zoom_button, GTK_ALIGN_CENTER);
    gtk_widget_set_tooltip_text(pw->zoom_button,
                                ns_i18n("Reset zoom (Ctrl+0)"));
    set_accessible_label(pw->zoom_button, ns_i18n("Reset zoom"));
    gtk_widget_set_visible(pw->zoom_button, FALSE);
    g_signal_connect(pw->zoom_button, "clicked",
                     G_CALLBACK(on_zoom_indicator_clicked), pw);

    pw->bookmarks_button = toolbar_button("southstar-bookmarks-symbolic",
                                          ns_i18n("Bookmarks"),
                                          G_CALLBACK(on_bookmarks_clicked), pw);
    GMenu *appmenu = g_menu_new();
    GMenu *sec_tabs = g_menu_new();
    menu_append_accel(sec_tabs, ns_i18n("New Tab"), "win.new-tab", NULL);
    menu_append_accel(sec_tabs, ns_i18n("New Private Tab"),
                      "win.new-private-tab", NULL);
    g_menu_append_section(appmenu, NULL, G_MENU_MODEL(sec_tabs));
    g_object_unref(sec_tabs);
    GMenu *sec_view = g_menu_new();
    menu_append_accel(sec_view, ns_i18n("Zoom In"), "win.zoom-in",
                      "<Ctrl>plus");
    menu_append_accel(sec_view, ns_i18n("Zoom Out"), "win.zoom-out",
                      "<Ctrl>minus");
    menu_append_accel(sec_view, ns_i18n("Reset Zoom"), "win.zoom-reset",
                      "<Ctrl>0");
    menu_append_accel(sec_view, ns_i18n("Full Screen"), "win.fullscreen", NULL);
    menu_append_accel(sec_view, ns_i18n("Find in Page"), "win.find", NULL);
    g_menu_append_section(appmenu, NULL, G_MENU_MODEL(sec_view));
    g_object_unref(sec_view);
    GMenu *sec_page = g_menu_new();
    menu_append_accel(sec_page, ns_i18n("Bookmark This Page"),
                      "win.bookmark-page", "<Ctrl>d");
    menu_append_accel(sec_page, ns_i18n("History"), "win.history", NULL);
    menu_append_accel(sec_page, ns_i18n("Downloads"), "win.downloads", NULL);
    menu_append_accel(sec_page, ns_i18n("Print…"), "win.print", "<Ctrl>P");
    menu_append_accel(sec_page, ns_i18n("Save Page as PDF…"), "win.save-pdf",
                      NULL);
    menu_append_accel(sec_page, ns_i18n("Save Page as Image…"),
                      "win.save-image", NULL);
    g_menu_append_section(appmenu, NULL, G_MENU_MODEL(sec_page));
    g_object_unref(sec_page);
    GMenu *sec_tools = g_menu_new();
    menu_append_accel(sec_tools, ns_i18n("Page Source"), "win.view-source",
                      "<Ctrl>u");
    menu_append_accel(sec_tools, ns_i18n("JavaScript Console"), "win.console",
                      "<Ctrl><Shift>j");
    menu_append_accel(sec_tools, ns_i18n("Task Manager"), "win.task-manager",
                      NULL);
    menu_append_accel(sec_tools, ns_i18n("Settings"), "win.settings", NULL);
    g_menu_append_section(appmenu, NULL, G_MENU_MODEL(sec_tools));
    g_object_unref(sec_tools);
    GMenu *appmenu_about = g_menu_new();
    g_menu_append(appmenu_about, ns_i18n("About Southstar"), "win.about");
    g_menu_append_section(appmenu, NULL, G_MENU_MODEL(appmenu_about));
    g_object_unref(appmenu_about);
    GtkWidget *menu_button = gtk_menu_button_new();
    ns_icon_menu_button_set(GTK_MENU_BUTTON(menu_button),
                            "southstar-menu-symbolic");
    gtk_widget_add_css_class(menu_button, "ns-nav-button");
    gtk_widget_set_valign(menu_button, GTK_ALIGN_CENTER);
    gtk_menu_button_set_menu_model(GTK_MENU_BUTTON(menu_button),
                                   G_MENU_MODEL(appmenu));
    ns_popover_menu_fit(GTK_WIDGET(gtk_menu_button_get_popover(
                            GTK_MENU_BUTTON(menu_button))));
    gtk_widget_set_tooltip_text(menu_button, ns_i18n("Menu"));
    set_accessible_label(menu_button, ns_i18n("Menu"));
    g_object_unref(appmenu);

    GtkWidget *logo = toolbar_button("southstar",
                                     ns_i18n("Visit the project page"),
                                     G_CALLBACK(on_logo_clicked), pw);
    gtk_widget_add_css_class(logo, "ns-logo");

    gtk_box_append(GTK_BOX(pw->toolbar), pw->back);
    gtk_box_append(GTK_BOX(pw->toolbar), pw->forward);
    gtk_box_append(GTK_BOX(pw->toolbar), pw->reload);
    gtk_box_append(GTK_BOX(pw->toolbar), home);
    gtk_box_append(GTK_BOX(pw->toolbar), pw->address);
    gtk_box_append(GTK_BOX(pw->toolbar), pw->zoom_button);
    gtk_box_append(GTK_BOX(pw->toolbar), pw->bookmarks_button);
    gtk_box_append(GTK_BOX(pw->toolbar), downloads);
    gtk_box_append(GTK_BOX(pw->toolbar), menu_button);
    gtk_box_append(GTK_BOX(pw->toolbar), logo);
    gtk_box_append(GTK_BOX(vbox), pw->toolbar);

    pw->notebook = gtk_notebook_new();
    gtk_notebook_set_show_tabs(GTK_NOTEBOOK(pw->notebook), FALSE);
    gtk_notebook_set_show_border(GTK_NOTEBOOK(pw->notebook), FALSE);
    gtk_notebook_set_scrollable(GTK_NOTEBOOK(pw->notebook), TRUE);
    gtk_widget_set_hexpand(pw->notebook, TRUE);
    gtk_widget_set_vexpand(pw->notebook, TRUE);
    g_signal_connect_after(pw->notebook, "switch-page",
                           G_CALLBACK(on_switch_page), pw);

    pw->status = gtk_label_new("");
    gtk_label_set_ellipsize(GTK_LABEL(pw->status), PANGO_ELLIPSIZE_MIDDLE);
    gtk_label_set_max_width_chars(GTK_LABEL(pw->status), 90);
    gtk_widget_add_css_class(pw->status, "ns-procstatus");
    gtk_widget_set_halign(pw->status, GTK_ALIGN_START);
    gtk_widget_set_valign(pw->status, GTK_ALIGN_END);
    gtk_widget_set_can_target(pw->status, FALSE);
    gtk_widget_set_visible(pw->status, FALSE);

    pw->fullscreen_notice = gtk_label_new("");
    gtk_label_set_ellipsize(GTK_LABEL(pw->fullscreen_notice),
                            PANGO_ELLIPSIZE_MIDDLE);
    gtk_label_set_max_width_chars(GTK_LABEL(pw->fullscreen_notice), 90);
    gtk_widget_add_css_class(pw->fullscreen_notice, "ns-fullscreen-notice");
    gtk_widget_set_halign(pw->fullscreen_notice, GTK_ALIGN_CENTER);
    gtk_widget_set_valign(pw->fullscreen_notice, GTK_ALIGN_START);
    gtk_widget_set_can_target(pw->fullscreen_notice, FALSE);
    gtk_widget_set_visible(pw->fullscreen_notice, FALSE);

    GtkWidget *page_overlay = gtk_overlay_new();
    gtk_overlay_set_child(GTK_OVERLAY(page_overlay), pw->notebook);
    gtk_overlay_add_overlay(GTK_OVERLAY(page_overlay), pw->status);
    gtk_overlay_add_overlay(GTK_OVERLAY(page_overlay), pw->fullscreen_notice);
    gtk_widget_set_hexpand(page_overlay, TRUE);
    gtk_widget_set_vexpand(page_overlay, TRUE);
    gtk_box_append(GTK_BOX(vbox), page_overlay);

    gtk_window_set_child(GTK_WINDOW(pw->window), vbox);
    install_shortcuts(pw);

    GtkSettings *settings = gtk_settings_get_default();
    if (settings) {
        pw->theme_watch[0] = g_signal_connect(
            settings, "notify::gtk-theme-name",
            G_CALLBACK(on_theme_changed), pw);
        pw->theme_watch[1] = g_signal_connect(
            settings, "notify::gtk-application-prefer-dark-theme",
            G_CALLBACK(on_theme_changed), pw);
    }

    return pw;
}

static void
act_about(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ProcWindow *pw = user_data;
    NsProcView *v = current_view(pw);
    if (v)
        ns_proc_view_load(v, "about:southstar");
}

static void
act_settings(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ProcWindow *pw = user_data;
    NsProcView *v = current_view(pw);
    if (v)
        ns_proc_view_load(v, "about:settings");
}

static void
act_history(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ProcWindow *pw = user_data;
    NsProcView *v = current_view(pw);
    if (v)
        ns_proc_view_load(v, "about:history");
}

static void
act_print(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ns_proc_view_print(current_view(user_data));
}

static void
act_save_pdf(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ns_proc_view_save_pdf(current_view(user_data));
}

static void
act_save_image(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ns_proc_view_save_image(current_view(user_data));
}

static void
act_fullscreen(GSimpleAction *action, GVariant *parameter, gpointer user_data)
{
    (void)action; (void)parameter;
    ProcWindow *pw = user_data;
    GtkWindow *win = GTK_WINDOW(pw->window);
    if (gtk_window_is_fullscreen(win)) {
        gtk_window_unfullscreen(win);
        gtk_window_maximize(win);
    } else {
        gtk_window_fullscreen(win);
    }
}

static void
on_bookmark_activate(GtkButton *button, gpointer user_data)
{
    ProcWindow *pw = user_data;
    const char *url = g_object_get_data(G_OBJECT(button), "ns-bm-url");
    NsProcView *v = current_view(pw);
    if (url && v)
        ns_proc_view_load(v, url);
    GtkWidget *pop = gtk_widget_get_ancestor(GTK_WIDGET(button),
                                             GTK_TYPE_POPOVER);
    if (pop)
        gtk_popover_popdown(GTK_POPOVER(pop));
}

static void
on_bookmark_remove(GtkButton *button, gpointer user_data)
{
    ProcWindow *pw = user_data;
    const char *url = g_object_get_data(G_OBJECT(button), "ns-bm-url");
    if (url && pw->bookmarks) {
        ns_bookmarks_remove(pw->bookmarks, url);
        GtkWidget *row = gtk_widget_get_parent(GTK_WIDGET(button));
        GtkWidget *list = row ? gtk_widget_get_parent(row) : NULL;
        if (list && row)
            gtk_box_remove(GTK_BOX(list), row);
        update_bookmark_indicator(pw);
    }
}

static void
on_add_bookmark(GtkButton *button, gpointer user_data)
{
    ProcWindow *pw = user_data;
    NsProcView *v = current_view(pw);
    if (!v || !pw->bookmarks)
        return;
    const char *url = ns_proc_view_url(v);
    if (!url || !*url)
        return;
    if (ns_bookmarks_contains(pw->bookmarks, url)) {
        ns_bookmarks_remove(pw->bookmarks, url);
        pw_set_status(pw, ns_i18n("Bookmark removed"));
    } else {
        ns_bookmarks_add(pw->bookmarks, url, ns_proc_view_title(v));
        pw_set_status(pw, ns_i18n("Bookmark added"));
    }
    update_bookmark_indicator(pw);
    GtkWidget *pop = gtk_widget_get_ancestor(GTK_WIDGET(button),
                                             GTK_TYPE_POPOVER);
    if (pop)
        gtk_popover_popdown(GTK_POPOVER(pop));
}

static GtkWidget *
build_bookmarks_popover(ProcWindow *pw)
{
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 4);
    gtk_widget_set_margin_top(box, 6);
    gtk_widget_set_margin_bottom(box, 6);
    gtk_widget_set_margin_start(box, 6);
    gtk_widget_set_margin_end(box, 6);
    gtk_widget_set_size_request(box, 320, -1);

    GtkWidget *add = gtk_button_new_with_label(
        current_page_bookmarked(pw) ? ns_i18n("Remove this bookmark")
                                    : ns_i18n("Bookmark this page"));
    g_signal_connect(add, "clicked", G_CALLBACK(on_add_bookmark), pw);
    gtk_box_append(GTK_BOX(box), add);
    gtk_box_append(GTK_BOX(box),
                   gtk_separator_new(GTK_ORIENTATION_HORIZONTAL));

    GtkWidget *scroll = gtk_scrolled_window_new();
    gtk_scrolled_window_set_policy(GTK_SCROLLED_WINDOW(scroll),
                                   GTK_POLICY_NEVER, GTK_POLICY_AUTOMATIC);
    gtk_scrolled_window_set_propagate_natural_height(
        GTK_SCROLLED_WINDOW(scroll), TRUE);
    gtk_scrolled_window_set_max_content_height(GTK_SCROLLED_WINDOW(scroll),
                                               320);
    GtkWidget *list = gtk_box_new(GTK_ORIENTATION_VERTICAL, 2);
    guint n = pw->bookmarks ? ns_bookmarks_count(pw->bookmarks) : 0;
    if (n == 0) {
        GtkWidget *empty = gtk_label_new(ns_i18n("No bookmarks yet"));
        gtk_widget_add_css_class(empty, "dim-label");
        gtk_widget_set_margin_top(empty, 12);
        gtk_widget_set_margin_bottom(empty, 12);
        gtk_box_append(GTK_BOX(list), empty);
    }
    for (guint i = 0; i < n; i++) {
        const ns_bookmark *bm = ns_bookmarks_get(pw->bookmarks, i);
        if (!bm || !bm->url) continue;
        GtkWidget *row = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
        GtkWidget *open = gtk_button_new_with_label(
            (bm->title && *bm->title) ? bm->title : bm->url);
        gtk_button_set_has_frame(GTK_BUTTON(open), FALSE);
        gtk_widget_set_hexpand(open, TRUE);
        gtk_widget_set_halign(open, GTK_ALIGN_START);
        gtk_widget_set_tooltip_text(open, bm->url);
        g_object_set_data_full(G_OBJECT(open), "ns-bm-url",
                               g_strdup(bm->url), g_free);
        g_signal_connect(open, "clicked", G_CALLBACK(on_bookmark_activate), pw);
        GtkWidget *del = gtk_button_new_from_icon_name("user-trash-symbolic");
        gtk_button_set_has_frame(GTK_BUTTON(del), FALSE);
        gtk_widget_set_tooltip_text(del, ns_i18n("Remove bookmark"));
        set_accessible_label(del, ns_i18n("Remove bookmark"));
        g_object_set_data_full(G_OBJECT(del), "ns-bm-url",
                               g_strdup(bm->url), g_free);
        g_signal_connect(del, "clicked", G_CALLBACK(on_bookmark_remove), pw);
        gtk_box_append(GTK_BOX(row), open);
        gtk_box_append(GTK_BOX(row), del);
        gtk_box_append(GTK_BOX(list), row);
    }
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), list);
    gtk_box_append(GTK_BOX(box), scroll);

    GtkWidget *pop = gtk_popover_new();
    gtk_popover_set_child(GTK_POPOVER(pop), box);
    return pop;
}

static gboolean
popover_unparent_idle(gpointer pop)
{
    if (gtk_widget_get_parent(GTK_WIDGET(pop)))
        gtk_widget_unparent(GTK_WIDGET(pop));
    g_object_unref(pop);
    return G_SOURCE_REMOVE;
}

static void
on_popover_closed_unparent(GtkPopover *pop, gpointer user_data)
{
    (void)user_data;
    g_idle_add(popover_unparent_idle, g_object_ref(pop));
}

static void
on_bookmarks_clicked(GtkButton *button, gpointer user_data)
{
    ProcWindow *pw = user_data;
    GtkWidget *pop = build_bookmarks_popover(pw);
    gtk_widget_set_parent(pop, GTK_WIDGET(button));
    gtk_popover_set_position(GTK_POPOVER(pop), GTK_POS_BOTTOM);
    g_signal_connect(pop, "closed",
                     G_CALLBACK(on_popover_closed_unparent), NULL);
    gtk_popover_popup(GTK_POPOVER(pop));
}

typedef struct {
    char    *url;
    char    *session_path;
    gboolean recover;
    gboolean private_mode;
} ProcAppCtx;

static gboolean
session_url_recoverable(const char *u)
{
    return u && (g_str_has_prefix(u, "http://") ||
                 g_str_has_prefix(u, "https://") ||
                 g_str_has_prefix(u, "ftp://") ||
                 g_str_has_prefix(u, "file://"));
}

static gboolean
write_session_cb(gpointer data)
{
    ProcWindow *pw = data;
    if (!pw->session_path)
        return G_SOURCE_REMOVE;
    GString *s = g_string_new(NULL);
    int n = gtk_notebook_get_n_pages(GTK_NOTEBOOK(pw->notebook));
    for (int i = 0; i < n; i++) {
        NsProcView *v = view_for_page(
            gtk_notebook_get_nth_page(GTK_NOTEBOOK(pw->notebook), i));
        if (v && ns_proc_view_is_private(v))
            continue;
        const char *u = v ? ns_proc_view_url(v) : NULL;
        if (session_url_recoverable(u)) {
            g_string_append(s, u);
            g_string_append_c(s, '\n');
        }
    }
    g_file_set_contents(pw->session_path, s->str, (gssize)s->len, NULL);
    g_string_free(s, TRUE);
    return G_SOURCE_CONTINUE;
}

static void
on_proc_activate(GtkApplication *app, gpointer user_data)
{
    ProcAppCtx *ctx = user_data;
    setlocale(LC_NUMERIC, "C");
    install_icon_search_paths();
    ns_icon_install_window_icon("southstar");
#ifdef __APPLE__
    ns_macos_set_dock_icon();
#endif
    install_chrome_css();
    ProcWindow *pw = proc_window_new(app);
    pw->session_path = g_strdup(ctx->session_path);
    gtk_window_present(GTK_WINDOW(pw->window));
    apply_color_scheme(pw);

    gboolean opened = FALSE;
    if (ctx->recover && ctx->session_path) {
        char *contents = NULL;
        if (g_file_get_contents(ctx->session_path, &contents, NULL, NULL)) {
            char **lines = g_strsplit(contents, "\n", -1);
            for (int i = 0; lines && lines[i]; i++) {
                if (session_url_recoverable(lines[i])) {
                    proc_window_add_tab(pw, lines[i], !opened);
                    opened = TRUE;
                }
            }
            g_strfreev(lines);
        }
        g_free(contents);
        if (opened)
            pw_set_status(pw, ns_i18n("Recovered the previous session after "
                                      "an unexpected exit"));
    }
    if (!opened)
        proc_window_add_tab_full(pw, ctx->url ? ctx->url : "about:start", TRUE,
                                 ctx->private_mode);

    if (pw->session_path)
        pw->session_timer = g_timeout_add_seconds(4, write_session_cb, pw);
}

static void
procapp_clear_cache_dir(const char *name, gint64 min_age_s)
{
    char *dir = g_build_filename(g_get_user_cache_dir(), "southstar",
                                 name, NULL);
    gint64 cutoff = g_get_real_time() / G_USEC_PER_SEC - min_age_s;
    GQueue *stack = g_queue_new();
    GPtrArray *dirs = g_ptr_array_new_with_free_func(g_free);
    g_queue_push_head(stack, g_strdup(dir));
    guint guard = 0;
    while (!g_queue_is_empty(stack) && guard++ < 100000) {
        char *d = g_queue_pop_head(stack);
        GDir *gd = g_dir_open(d, 0, NULL);
        if (gd) {
            const char *e;
            while ((e = g_dir_read_name(gd))) {
                char *child = g_build_filename(d, e, NULL);
                if (g_file_test(child, G_FILE_TEST_IS_SYMLINK) ||
                    !g_file_test(child, G_FILE_TEST_IS_DIR)) {
                    GStatBuf st;
                    if (min_age_s <= 0 ||
                        (g_lstat(child, &st) == 0 && st.st_mtime < cutoff))
                        g_unlink(child);
                    g_free(child);
                } else {
                    g_queue_push_head(stack, child);
                }
            }
            g_dir_close(gd);
        }
        g_ptr_array_add(dirs, d);
    }
    for (guint i = dirs->len; i > 1; i--)
        g_rmdir(g_ptr_array_index(dirs, i - 1));
    g_queue_free_full(stack, g_free);
    g_ptr_array_free(dirs, TRUE);
    g_free(dir);
}

static void
procapp_clear_http_caches(gboolean at_exit)
{
    static const char *const object_dirs[] = {
        "cache", "jsbc", "webfonts", "frames",
    };
    static const char *const stream_dirs[] = { "msaudio", "msvideo" };
    for (gsize i = 0; i < G_N_ELEMENTS(object_dirs); i++)
        procapp_clear_cache_dir(object_dirs[i], at_exit ? 0 : 3600);
    for (gsize i = 0; i < G_N_ELEMENTS(stream_dirs); i++)
        procapp_clear_cache_dir(stream_dirs[i], 3600);
}

int
ns_procapp_run(const char *startup_url, const char *session_path,
               gboolean recover, gboolean private_mode)
{
    if (ns_proc_video_helper_available())
        g_setenv("NS_VIDEO_HELPER", "1", TRUE);
    procapp_clear_http_caches(FALSE);
    if (!private_mode)
        ns_history_init();
    ProcAppCtx ctx = {
        .url = g_strdup(startup_url),
        .session_path = g_strdup(session_path),
        .recover = recover,
        .private_mode = private_mode,
    };
    GtkApplication *app =
        gtk_application_new(NS_PROC_APP_ID, G_APPLICATION_NON_UNIQUE);
    g_signal_connect(app, "activate", G_CALLBACK(on_proc_activate), &ctx);
    int status = g_application_run(G_APPLICATION(app), 0, NULL);
    g_object_unref(app);
    ns_history_shutdown();
    procapp_clear_http_caches(TRUE);
    g_free(ctx.url);
    g_free(ctx.session_path);
    return status;
}
