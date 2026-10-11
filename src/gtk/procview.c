/* Southstar — GTK thin client over the out-of-process renderer (rproc). */

#include "procview.h"
#include "procview_state.h"
#include "i18n.h"
#include "pagelayers.h"

#include "libsouthstar.h"
#include "proc_limits.h"
#include "../print.h"
#include "rproc_http.h"
#include "rproc_inproc.h"
#include "net.h"

#include <cairo.h>
#include <glib/gstdio.h>
#include <math.h>
#include <stdlib.h>
#include <string.h>
#ifdef G_OS_WIN32
#include <windows.h>
#endif

#ifndef G_OS_WIN32
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#endif

static NsProcView *pv_ref(NsProcView *v) { g_ref_count_inc(&v->rc); return v; }

static void
set_accessible_label(GtkWidget *w, const char *label)
{
    gtk_accessible_update_property(GTK_ACCESSIBLE(w),
                                   GTK_ACCESSIBLE_PROPERTY_LABEL, label, -1);
}

static void
pv_free(NsProcView *v)
{
    ns_pv_media_free(v->media);
    v->media = NULL;
    if (v->queue) {
        Req *r;
        while ((r = g_async_queue_try_pop(v->queue))) {
            g_free(r->url);
            g_free(r->key);
            g_free(r->code);
            g_free(r->query);
            g_free(r->export_dest);
            g_free(r->paths);
            g_free(r->have);
            g_free(r->hold);
            g_free(r);
        }
        g_async_queue_unref(v->queue);
    }
    g_clear_object(&v->frame);
    ns_page_layers_free(v->layers);
    if (v->favicon)
        g_object_unref(v->favicon);
    v->favicon = NULL;
    if (v->ctx_popover)
        gtk_widget_unparent(v->ctx_popover);
    if (v->ctx_actions)
        g_object_unref(v->ctx_actions);
    g_free(v->ctx_link);
    if (v->history)
        g_ptr_array_unref(v->history);
    g_free(v->renderer_path);
    g_free(v->current_url);
    g_free(v->current_title);
    g_free(v->remote_ip);
    g_free(v->deferred_url);
    g_free(v->perm_origin);
    if (v->hourglass_cursor)
        g_object_unref(v->hourglass_cursor);
    g_mutex_clear(&v->proc_lock);
    g_free(v);
}

static void pv_unref(NsProcView *v) { if (g_ref_count_dec(&v->rc)) pv_free(v); }

NsProcView *
ns_proc_view_ref(NsProcView *v)
{
    return pv_ref(v);
}

void
ns_proc_view_unref(NsProcView *v)
{
    pv_unref(v);
}

GtkWidget *
ns_proc_view_area(NsProcView *v)
{
    return v->area;
}

NsPvMedia *
ns_proc_view_media(NsProcView *v)
{
    return v ? v->media : NULL;
}

static void build_search_bar(NsProcView *v);

static void
on_perm_allow(GtkButton *btn, gpointer data)
{
    (void)btn;
    ns_pv_perm_resolve(data, TRUE);
}

static void
on_perm_deny(GtkButton *btn, gpointer data)
{
    (void)btn;
    ns_pv_perm_resolve(data, FALSE);
}

typedef struct {
    GPtrArray      *pages;
    ns_print_setup  setup;
    double          raster_scale;
    char           *title;
} PrintJob;

static void
print_job_free(gpointer data)
{
    PrintJob *job = data;
    for (guint i = 0; i < job->pages->len; i++)
        cairo_surface_destroy(g_ptr_array_index(job->pages, i));
    g_ptr_array_free(job->pages, TRUE);
    g_free(job->title);
    g_free(job);
}

static void
on_print_begin(GtkPrintOperation *op, GtkPrintContext *ctx, gpointer data)
{
    (void)ctx;
    PrintJob *job = data;
    gtk_print_operation_set_n_pages(op, (int)job->pages->len);
}

static void
on_print_draw(GtkPrintOperation *op, GtkPrintContext *ctx, int page_nr,
              gpointer data)
{
    (void)op;
    PrintJob *job = data;
    if (page_nr < 0 || (guint)page_nr >= job->pages->len)
        return;
    cairo_t *cr = gtk_print_context_get_cairo_context(ctx);
    double surface_w = gtk_print_context_get_width(ctx);
    double scale = job->setup.width > 0 ? surface_w / job->setup.width : 1.0;
    scale /= job->raster_scale > 0 ? job->raster_scale : 1.0;
    cairo_save(cr);
    cairo_scale(cr, scale, scale);
    cairo_set_source_surface(cr, g_ptr_array_index(job->pages, page_nr), 0, 0);
    cairo_paint(cr);
    cairo_restore(cr);
}

void
ns_pv_print_run(NsProcView *v, GPtrArray *pages, const ns_print_setup *setup,
          double raster_scale)
{
    PrintJob *job = g_new0(PrintJob, 1);
    job->pages = pages;
    job->setup = *setup;
    job->raster_scale = raster_scale > 0 ? raster_scale : 1.0;
    job->title = g_strdup((v->current_title && *v->current_title)
                          ? v->current_title : "page");

    GtkPageSetup *page_setup = gtk_page_setup_new();
    GtkPaperSize *paper = gtk_paper_size_new_custom(
        "southstar", "Southstar", setup->width * 72.0 / 96.0,
        setup->height * 72.0 / 96.0, GTK_UNIT_POINTS);
    gtk_page_setup_set_paper_size(page_setup, paper);
    gtk_page_setup_set_orientation(page_setup,
        setup->width > setup->height ? GTK_PAGE_ORIENTATION_LANDSCAPE
                                     : GTK_PAGE_ORIENTATION_PORTRAIT);
    gtk_paper_size_free(paper);

    GtkPrintOperation *op = gtk_print_operation_new();
    gtk_print_operation_set_default_page_setup(op, page_setup);
    gtk_print_operation_set_use_full_page(op, TRUE);
    gtk_print_operation_set_unit(op, GTK_UNIT_POINTS);
    gtk_print_operation_set_job_name(op, job->title);
    gtk_print_operation_set_embed_page_setup(op, TRUE);
    g_signal_connect(op, "begin-print", G_CALLBACK(on_print_begin), job);
    g_signal_connect(op, "draw-page", G_CALLBACK(on_print_draw), job);
    g_object_set_data_full(G_OBJECT(op), "ns-print-job", job, print_job_free);

    GtkRoot *root = v->area ? gtk_widget_get_root(v->area) : NULL;
    GError *err = NULL;
    gtk_print_operation_run(op, GTK_PRINT_OPERATION_ACTION_PRINT_DIALOG,
                            GTK_WINDOW(root), &err);
    if (err) {
        ns_pv_post_emit(v, NS_PROC_EVT_STATUS, err->message);
        g_error_free(err);
    }
    g_object_unref(page_setup);
    g_object_unref(op);
}

typedef struct { NsProcView *view; gboolean pdf; } ExportCtx;

static void
on_save_dialog_done(GObject *src, GAsyncResult *res, gpointer ud)
{
    ExportCtx *c = ud;
    NsProcView *v = c->view;
    GError *err = NULL;
    GFile *file = gtk_file_dialog_save_finish(GTK_FILE_DIALOG(src), res, &err);
    if (file) {
        char *dest = g_file_get_path(file);
        if (dest && v->opened) {
            static int export_counter = 0;
            char *base = g_strdup_printf(
                "southstar-export-%" G_GINT64_FORMAT "-%d.%s",
                g_get_monotonic_time(), ++export_counter,
                c->pdf ? "pdf" : "png");
            Req *req = g_new0(Req, 1);
            req->type = REQ_EXPORT;
            req->url = g_build_filename(g_get_user_runtime_dir(), base, NULL);
            req->export_dest = g_strdup(dest);
            ns_pv_push_req(v, req);
            g_free(base);
        }
        g_free(dest);
        g_object_unref(file);
    }
    g_clear_error(&err);
    pv_unref(v);
    g_free(c);
}

void
ns_pv_view_save(NsProcView *v, gboolean pdf)
{
    if (!v->opened || !v->area)
        return;
    GtkRoot *root = gtk_widget_get_root(v->area);
    GtkFileDialog *dialog = gtk_file_dialog_new();
    gtk_file_dialog_set_title(dialog,
        pdf ? ns_i18n("Save page as PDF")
                                      : ns_i18n("Save page as PNG"));
    const char *t = (v->current_title && *v->current_title)
        ? v->current_title : "page";
    char *name = g_strdup_printf("%s.%s", t, pdf ? "pdf" : "png");
    g_strdelimit(name, "/", '_');
    gtk_file_dialog_set_initial_name(dialog, name);
    ExportCtx *c = g_new0(ExportCtx, 1);
    c->view = pv_ref(v);
    c->pdf = pdf;
    gtk_file_dialog_save(dialog, GTK_WINDOW(root), NULL,
                         on_save_dialog_done, c);
    g_object_unref(dialog);
    g_free(name);
}

typedef enum {
    EDIT_NONE, EDIT_CUT, EDIT_COPY, EDIT_PASTE, EDIT_SELECT_ALL
} EditOp;

static void
ctx_set_clipboard(NsProcView *v, const char *text, const char *status)
{
    if (!text || !*text || !v->area)
        return;
    gdk_clipboard_set_text(gtk_widget_get_clipboard(v->area), text);
    ns_pv_post_emit(v, NS_PROC_EVT_STATUS, status);
}

static void
on_ctx_back(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_proc_view_back(ud); }

static void
on_ctx_forward(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_proc_view_forward(ud); }

static void
on_ctx_reload(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_proc_view_reload(ud); }

static void
on_ctx_copy_url(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    ctx_set_clipboard(v, v->current_url, ns_i18n("Copied page address"));
}

static void
on_ctx_open_newtab(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    if (v->ctx_link && *v->ctx_link)
        ns_pv_post_emit(v, NS_PROC_EVT_NEWTAB, v->ctx_link);
}

static void
on_ctx_open_link(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    if (v->ctx_link && *v->ctx_link)
        ns_proc_view_load(v, v->ctx_link);
}

static void
on_ctx_copy_link(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    ctx_set_clipboard(v, v->ctx_link, ns_i18n("Copied link address"));
}

static void
on_ctx_cut(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_run_edit_op(ud, EDIT_CUT); }

static void
on_ctx_copy_sel(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_run_edit_op(ud, EDIT_COPY); }

static void
on_ctx_paste(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_run_edit_op(ud, EDIT_PASTE); }

static void
on_ctx_select_all(GSimpleAction *a, GVariant *p, gpointer ud)
{
    (void)a; (void)p;
    NsProcView *v = ud;
    v->has_selection = TRUE;
    ns_pv_start_select(v, 3, 0, 0);
}

void
ns_proc_view_print(NsProcView *view)
{
    if (!view || !view->opened)
        return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_PRINT;
    ns_pv_push_req(view, req);
}

void
ns_proc_view_save_pdf(NsProcView *view)
{ if (view) ns_pv_view_save(view, TRUE); }

void
ns_proc_view_save_image(NsProcView *view)
{ if (view) ns_pv_view_save(view, FALSE); }

static void
on_ctx_save_pdf(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_view_save(ud, TRUE); }

static void
on_ctx_save_png(GSimpleAction *a, GVariant *p, gpointer ud)
{ (void)a; (void)p; ns_pv_view_save(ud, FALSE); }

static void
ctx_action_enable(NsProcView *v, const char *name, gboolean on)
{
    GAction *act = g_action_map_lookup_action(G_ACTION_MAP(v->ctx_actions),
                                              name);
    if (act)
        g_simple_action_set_enabled(G_SIMPLE_ACTION(act), on);
}

static void
ctx_install_actions(NsProcView *v)
{
    static const GActionEntry entries[] = {
        { "back",        on_ctx_back,        NULL, NULL, NULL, {0} },
        { "forward",     on_ctx_forward,     NULL, NULL, NULL, {0} },
        { "reload",      on_ctx_reload,      NULL, NULL, NULL, {0} },
        { "copy-url",    on_ctx_copy_url,    NULL, NULL, NULL, {0} },
        { "open-link",   on_ctx_open_link,   NULL, NULL, NULL, {0} },
        { "open-newtab", on_ctx_open_newtab, NULL, NULL, NULL, {0} },
        { "copy-link",   on_ctx_copy_link,   NULL, NULL, NULL, {0} },
        { "cut",         on_ctx_cut,         NULL, NULL, NULL, {0} },
        { "copy-sel",    on_ctx_copy_sel,    NULL, NULL, NULL, {0} },
        { "paste",       on_ctx_paste,       NULL, NULL, NULL, {0} },
        { "select-all",  on_ctx_select_all,  NULL, NULL, NULL, {0} },
        { "save-pdf",    on_ctx_save_pdf,    NULL, NULL, NULL, {0} },
        { "save-png",    on_ctx_save_png,    NULL, NULL, NULL, {0} },
    };
    v->ctx_actions = g_simple_action_group_new();
    g_action_map_add_action_entries(G_ACTION_MAP(v->ctx_actions), entries,
                                    G_N_ELEMENTS(entries), v);
    gtk_widget_insert_action_group(v->area, "ctx",
                                   G_ACTION_GROUP(v->ctx_actions));
}

static gboolean
popover_present_idle(gpointer data)
{
    GtkWidget *pop = data;
    if (gtk_widget_get_mapped(pop))
        gtk_popover_present(GTK_POPOVER(pop));
    g_object_unref(pop);
    return G_SOURCE_REMOVE;
}

static void
on_popover_menu_mapped(GtkWidget *pop, gpointer user_data)
{
    (void)user_data;
    g_idle_add(popover_present_idle, g_object_ref(pop));
}

void
ns_popover_menu_fit(GtkWidget *popover)
{
    if (!GTK_IS_POPOVER(popover)) return;
    g_signal_connect(popover, "map", G_CALLBACK(on_popover_menu_mapped), NULL);
}

static GMenu *
ctx_field_menu(NsProcView *v, int edit)
{
    gboolean selected = (edit & NS_BROWSER_EDIT_SELECTION) != 0;
    gboolean writable = (edit & NS_BROWSER_EDIT_WRITABLE) != 0;
    ctx_action_enable(v, "cut", writable && selected);
    ctx_action_enable(v, "copy-sel", selected);
    ctx_action_enable(v, "paste", writable);

    GMenu *menu = g_menu_new();
    GMenu *clip = g_menu_new();
    g_menu_append(clip, ns_i18n("Cut"), "ctx.cut");
    g_menu_append(clip, ns_i18n("Copy"), "ctx.copy-sel");
    g_menu_append(clip, ns_i18n("Paste"), "ctx.paste");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(clip));
    g_object_unref(clip);
    GMenu *all = g_menu_new();
    g_menu_append(all, ns_i18n("Select All"), "ctx.select-all");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(all));
    g_object_unref(all);
    return menu;
}

static GMenu *
ctx_page_menu(NsProcView *v)
{
    ctx_action_enable(v, "back", ns_proc_view_can_back(v));
    ctx_action_enable(v, "forward", ns_proc_view_can_forward(v));
    ctx_action_enable(v, "open-link", v->ctx_link != NULL);
    ctx_action_enable(v, "open-newtab", v->ctx_link != NULL);
    ctx_action_enable(v, "copy-link", v->ctx_link != NULL);
    ctx_action_enable(v, "copy-sel", v->has_selection);

    GMenu *menu = g_menu_new();
    if (v->ctx_link) {
        GMenu *s = g_menu_new();
        g_menu_append(s, ns_i18n("Open Link"), "ctx.open-link");
        if (!ns_rproc_single_process_enabled())
            g_menu_append(s, ns_i18n("Open Link in New Tab"), "ctx.open-newtab");
        g_menu_append(s, ns_i18n("Copy Link Address"), "ctx.copy-link");
        g_menu_append_section(menu, NULL, G_MENU_MODEL(s));
        g_object_unref(s);
    }
    if (v->has_selection) {
        GMenu *s = g_menu_new();
        g_menu_append(s, ns_i18n("Copy"), "ctx.copy-sel");
        g_menu_append_section(menu, NULL, G_MENU_MODEL(s));
        g_object_unref(s);
    }
    GMenu *nav = g_menu_new();
    g_menu_append(nav, ns_i18n("Back"), "ctx.back");
    g_menu_append(nav, ns_i18n("Forward"), "ctx.forward");
    g_menu_append(nav, ns_i18n("Reload"), "ctx.reload");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(nav));
    g_object_unref(nav);
    GMenu *page = g_menu_new();
    g_menu_append(page, ns_i18n("Select All"), "ctx.select-all");
    g_menu_append(page, ns_i18n("Copy Page Address"), "ctx.copy-url");
    g_menu_append(page, ns_i18n("Save Page as PDF…"), "ctx.save-pdf");
    g_menu_append(page, ns_i18n("Save Page as Image…"), "ctx.save-png");
    g_menu_append_section(menu, NULL, G_MENU_MODEL(page));
    g_object_unref(page);
    return menu;
}

void
ns_pv_show_context_menu(NsProcView *v, const char *href, int edit)
{
    g_free(v->ctx_link);
    v->ctx_link = (href && *href) ? g_strdup(href) : NULL;

    GMenu *menu = (edit & NS_BROWSER_EDIT_FIELD) ? ctx_field_menu(v, edit)
                                                 : ctx_page_menu(v);
    if (v->ctx_popover)
        gtk_widget_unparent(v->ctx_popover);
    v->ctx_popover = gtk_popover_menu_new_from_model(G_MENU_MODEL(menu));
    g_object_unref(menu);
    gtk_widget_set_parent(v->ctx_popover, v->area);
    ns_popover_menu_fit(v->ctx_popover);
    gtk_popover_set_has_arrow(GTK_POPOVER(v->ctx_popover), FALSE);
    gtk_popover_set_pointing_to(GTK_POPOVER(v->ctx_popover),
        &(GdkRectangle){ (int)v->ctx_x, (int)v->ctx_y, 1, 1 });
    gtk_popover_popup(GTK_POPOVER(v->ctx_popover));
}

void
ns_pv_request_find(NsProcView *v, int direction)
{
    if (!v->opened || !v->search_entry)
        return;
    const char *q = gtk_editable_get_text(GTK_EDITABLE(v->search_entry));
    Req *req = g_new0(Req, 1);
    req->type = REQ_FIND;
    req->seq = ++v->find_seq;
    req->query = g_strdup(q ? q : "");
    req->find_dir = direction;
    req->find_from_y = v->scroll_y;
    req->find_case = v->find_case;
    ns_pv_push_req(v, req);
}

void
ns_pv_search_open(NsProcView *v)
{
    if (!v->search_revealer)
        return;
    gtk_revealer_set_reveal_child(GTK_REVEALER(v->search_revealer), TRUE);
    gtk_widget_grab_focus(v->search_entry);
    gtk_editable_select_region(GTK_EDITABLE(v->search_entry), 0, -1);
    const char *q = gtk_editable_get_text(GTK_EDITABLE(v->search_entry));
    if (q && *q)
        ns_pv_request_find(v, 0);
}

static void
search_close(NsProcView *v)
{
    if (!v->search_revealer)
        return;
    gtk_revealer_set_reveal_child(GTK_REVEALER(v->search_revealer), FALSE);
    gtk_label_set_text(GTK_LABEL(v->search_label), "");
    Req *req = g_new0(Req, 1);
    req->type = REQ_FIND;
    req->seq = ++v->find_seq;
    req->query = g_strdup("");
    req->find_from_y = v->scroll_y;
    ns_pv_push_req(v, req);
    gtk_widget_grab_focus(v->area);
}

void
ns_proc_view_find_open(NsProcView *v)
{
    ns_pv_search_open(v);
}

void
ns_proc_view_set_color_scheme(NsProcView *v, gboolean dark)
{
    if (!v) return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_COLOR_SCHEME;
    req->mods = dark ? 1 : 0;
    ns_pv_push_req(v, req);
}

static void
on_search_changed(GtkSearchEntry *e, gpointer data)
{ (void)e; ns_pv_request_find(data, 0); }

static void
on_search_next(GtkWidget *w, gpointer data)
{ (void)w; ns_pv_request_find(data, 1); }

static void
on_search_prev(GtkWidget *w, gpointer data)
{ (void)w; ns_pv_request_find(data, 2); }

gboolean
ns_proc_view_find_close(NsProcView *view)
{
    if (!view || !view->search_revealer ||
        !gtk_revealer_get_reveal_child(GTK_REVEALER(view->search_revealer)))
        return FALSE;
    search_close(view);
    return TRUE;
}

static void
on_search_stop(GtkSearchEntry *e, gpointer data)
{ (void)e; search_close(data); }

static void
on_search_close_clicked(GtkButton *b, gpointer data)
{ (void)b; search_close(data); }

static void
on_area_destroy(GtkWidget *widget, gpointer data)
{
    NsProcView *v = data;
    ns_pv_area_set_view(widget, NULL);
    v->closed = TRUE;
    g_clear_object(&v->im);
    ns_pv_disarm_anim(v);
    if (v->hover_after_scroll_id) {
        g_source_remove(v->hover_after_scroll_id);
        v->hover_after_scroll_id = 0;
    }
    if (v->console_poll_id) {
        g_source_remove(v->console_poll_id);
        v->console_poll_id = 0;
    }
    if (v->console_window) {
        GtkWidget *win = v->console_window;
        v->console_window = NULL;
        v->console_notebook = NULL;
        v->console_entry = NULL;
        v->console_view = NULL;
        v->console_buffer = NULL;
        v->net_view = NULL;
        v->net_buffer = NULL;
        v->perf_view = NULL;
        v->perf_buffer = NULL;
        v->layout_view = NULL;
        v->layout_buffer = NULL;
        v->elements_view = NULL;
        v->elements_buffer = NULL;
        v->inspect_entry = NULL;
        gtk_window_destroy(GTK_WINDOW(win));
    }
    if (v->ctx_popover) {
        gtk_widget_unparent(v->ctx_popover);
        v->ctx_popover = NULL;
    }
    v->area = NULL;
    v->perm_revealer = NULL;
    v->perm_label = NULL;
    Req *req = g_new0(Req, 1);
    req->type = REQ_QUIT;
    ns_pv_push_req(v, req);
    /* Unblock the worker if it is mid-request to a wedged renderer, so the join
     * below can't stall the main loop for up to the 30 s IPC read timeout (which
     * would also trip the watchdog heartbeat and restart the whole shell). */
    g_mutex_lock(&v->proc_lock);
    if (v->proc)
        ns_rproc_http_interrupt(v->proc);
    g_mutex_unlock(&v->proc_lock);
    if (v->thread) {
        g_thread_join(v->thread);
        v->thread = NULL;
    }
    pv_unref(v);
}

void
ns_pv_console_append(NsProcView *v, const char *text)
{
    if (!v->console_buffer || !text || !*text)
        return;
    GtkTextIter end;
    gtk_text_buffer_get_end_iter(v->console_buffer, &end);
    gtk_text_buffer_insert(v->console_buffer, &end, text, -1);
    if (v->console_view) {
        gtk_text_buffer_get_end_iter(v->console_buffer, &end);
        GtkTextMark *m = gtk_text_buffer_create_mark(v->console_buffer, NULL,
                                                     &end, FALSE);
        gtk_text_view_scroll_mark_onscreen(GTK_TEXT_VIEW(v->console_view), m);
        gtk_text_buffer_delete_mark(v->console_buffer, m);
    }
}

static gboolean
console_poll_cb(gpointer data)
{
    NsProcView *v = data;
    if (!v->console_open || !v->opened)
        return G_SOURCE_CONTINUE;
    Req *req = g_new0(Req, 1);
    req->type = REQ_CONSOLE;
    ns_pv_push_req(v, req);
    return G_SOURCE_CONTINUE;
}

static void
on_console_eval(GtkEntry *entry, gpointer data)
{
    NsProcView *v = data;
    const char *src = gtk_editable_get_text(GTK_EDITABLE(entry));
    if (!src || !*src || !v->opened)
        return;
    char *echo = g_strdup_printf("> %s\n", src);
    ns_pv_console_append(v, echo);
    g_free(echo);
    Req *req = g_new0(Req, 1);
    req->type = REQ_EVAL;
    req->query = g_strdup(src);
    ns_pv_push_req(v, req);
    gtk_editable_set_text(GTK_EDITABLE(entry), "");
}

static void build_console_window(NsProcView *v);

void
ns_pv_console_set_open(NsProcView *v, gboolean open)
{
    if (open && !v->console_window)
        build_console_window(v);
    if (!v->console_window)
        return;
    v->console_open = open;
    if (open) {
        GtkRoot *root = gtk_widget_get_root(v->area);
        if (GTK_IS_WINDOW(root))
            gtk_window_set_transient_for(GTK_WINDOW(v->console_window),
                                         GTK_WINDOW(root));
        gtk_window_present(GTK_WINDOW(v->console_window));
        if (!v->console_poll_id)
            v->console_poll_id = g_timeout_add(NS_PROC_CONSOLE_POLL_MS, console_poll_cb, v);
        gtk_widget_grab_focus(v->console_entry);
    } else {
        gtk_widget_set_visible(v->console_window, FALSE);
        if (v->console_poll_id) {
            g_source_remove(v->console_poll_id);
            v->console_poll_id = 0;
        }
        if (v->area)
            gtk_widget_grab_focus(v->area);
    }
}

static void
console_request_dump(NsProcView *v, int tab)
{
    const char *kind = NULL;
    switch (tab) {
    case DEV_TAB_NETWORK:     kind = "network";     break;
    case DEV_TAB_PERFORMANCE: kind = "performance"; break;
    case DEV_TAB_LAYOUT:      kind = "layout";      break;
    case DEV_TAB_ELEMENTS:    kind = "dom";         break;
    default: return;
    }
    if (!v->opened)
        return;
    Req *req = g_new0(Req, 1);
    req->type = REQ_DUMP;
    req->dump_tab = tab;
    req->query = g_strdup(kind);
    ns_pv_push_req(v, req);
}

static int
console_current_tab(NsProcView *v)
{
    if (!v->console_notebook)
        return DEV_TAB_CONSOLE;
    return gtk_notebook_get_current_page(GTK_NOTEBOOK(v->console_notebook));
}

static void
on_console_clear(GtkButton *b, gpointer data)
{
    (void)b;
    NsProcView *v = data;
    GtkTextBuffer *buf = NULL;
    switch (console_current_tab(v)) {
    case DEV_TAB_CONSOLE:     buf = v->console_buffer;  break;
    case DEV_TAB_NETWORK:     buf = v->net_buffer;      break;
    case DEV_TAB_PERFORMANCE: buf = v->perf_buffer;     break;
    case DEV_TAB_LAYOUT:      buf = v->layout_buffer;   break;
    case DEV_TAB_ELEMENTS:    buf = v->elements_buffer; break;
    default: break;
    }
    if (buf)
        gtk_text_buffer_set_text(buf, "", 0);
}

static void
on_console_refresh(GtkButton *b, gpointer data)
{
    (void)b;
    NsProcView *v = data;
    console_request_dump(v, console_current_tab(v));
}

static void
on_console_notebook_switch(GtkNotebook *nb, GtkWidget *page, guint num,
                           gpointer data)
{
    (void)nb;
    (void)page;
    NsProcView *v = data;
    console_request_dump(v, (int)num);
}

static void
on_inspect_activate(GtkEntry *entry, gpointer data)
{
    NsProcView *v = data;
    const char *sel = gtk_editable_get_text(GTK_EDITABLE(entry));
    if (!sel || !*sel || !v->opened)
        return;
    char *esc = g_strescape(sel, NULL);
    char *src = g_strdup_printf(
        "(function(){try{var e=document.querySelector(\"%s\");"
        "if(!e)return \"\";var s=e.outerHTML;"
        "return s.length>20000?s.slice(0,20000)+\"\\n\\u2026(truncated)\":s;}"
        "catch(err){return \"Error: \"+err;}})()",
        esc);
    g_free(esc);
    Req *req = g_new0(Req, 1);
    req->type = REQ_EVAL;
    req->inspect = TRUE;
    req->dump_tab = DEV_TAB_ELEMENTS;
    req->query = src;
    ns_pv_push_req(v, req);
}

static gboolean
on_console_close_request(GtkWindow *win, gpointer data)
{
    (void)win;
    ns_pv_console_set_open(data, FALSE);
    return TRUE;
}

static GtkWidget *
console_make_view(GtkWidget **out_view, GtkTextBuffer **out_buffer,
                  gboolean wrap)
{
    GtkWidget *view = gtk_text_view_new();
    gtk_text_view_set_editable(GTK_TEXT_VIEW(view), FALSE);
    gtk_text_view_set_monospace(GTK_TEXT_VIEW(view), TRUE);
    gtk_text_view_set_cursor_visible(GTK_TEXT_VIEW(view), FALSE);
    gtk_text_view_set_wrap_mode(GTK_TEXT_VIEW(view),
                                wrap ? GTK_WRAP_WORD_CHAR : GTK_WRAP_NONE);
    *out_view = view;
    *out_buffer = gtk_text_view_get_buffer(GTK_TEXT_VIEW(view));
    GtkWidget *scroll = gtk_scrolled_window_new();
    gtk_scrolled_window_set_child(GTK_SCROLLED_WINDOW(scroll), view);
    gtk_widget_set_vexpand(scroll, TRUE);
    return scroll;
}

static void
console_add_tab(NsProcView *v, GtkWidget *child, const char *title)
{
    gtk_notebook_append_page(GTK_NOTEBOOK(v->console_notebook), child,
                             gtk_label_new(ns_i18n(title)));
}

static void
build_console_window(NsProcView *v)
{
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);

    GtkWidget *header = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    gtk_widget_add_css_class(header, "toolbar");
    GtkWidget *spacer = gtk_label_new("");
    gtk_widget_set_hexpand(spacer, TRUE);
    GtkWidget *refresh = gtk_button_new_from_icon_name("view-refresh-symbolic");
    gtk_widget_set_tooltip_text(refresh, ns_i18n("Refresh"));
    set_accessible_label(refresh, ns_i18n("Refresh"));
    g_signal_connect(refresh, "clicked", G_CALLBACK(on_console_refresh), v);
    GtkWidget *clear = gtk_button_new_from_icon_name("edit-clear-symbolic");
    gtk_widget_set_tooltip_text(clear, ns_i18n("Clear"));
    set_accessible_label(clear, ns_i18n("Clear"));
    g_signal_connect(clear, "clicked", G_CALLBACK(on_console_clear), v);
    gtk_box_append(GTK_BOX(header), spacer);
    gtk_box_append(GTK_BOX(header), refresh);
    gtk_box_append(GTK_BOX(header), clear);

    v->console_notebook = gtk_notebook_new();
    gtk_widget_set_vexpand(v->console_notebook, TRUE);

    GtkWidget *console_page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *console_scroll =
        console_make_view(&v->console_view, &v->console_buffer, TRUE);
    v->console_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(GTK_ENTRY(v->console_entry),
                                   ns_i18n("Evaluate JavaScript and press Enter"));
    g_signal_connect(v->console_entry, "activate",
                     G_CALLBACK(on_console_eval), v);
    gtk_box_append(GTK_BOX(console_page), console_scroll);
    gtk_box_append(GTK_BOX(console_page), v->console_entry);
    console_add_tab(v, console_page, "Console");

    console_add_tab(v, console_make_view(&v->net_view, &v->net_buffer, FALSE),
                    "Network");
    console_add_tab(v, console_make_view(&v->perf_view, &v->perf_buffer, FALSE),
                    "Performance");
    console_add_tab(v,
                    console_make_view(&v->layout_view, &v->layout_buffer, FALSE),
                    "Layout");

    GtkWidget *elements_page = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    GtkWidget *elements_scroll =
        console_make_view(&v->elements_view, &v->elements_buffer, FALSE);
    v->inspect_entry = gtk_entry_new();
    gtk_entry_set_placeholder_text(
        GTK_ENTRY(v->inspect_entry),
        ns_i18n("Inspect: CSS selector, then Enter"));
    g_signal_connect(v->inspect_entry, "activate",
                     G_CALLBACK(on_inspect_activate), v);
    gtk_box_append(GTK_BOX(elements_page), elements_scroll);
    gtk_box_append(GTK_BOX(elements_page), v->inspect_entry);
    console_add_tab(v, elements_page, "Elements");

    g_signal_connect(v->console_notebook, "switch-page",
                     G_CALLBACK(on_console_notebook_switch), v);

    gtk_box_append(GTK_BOX(box), header);
    gtk_box_append(GTK_BOX(box), v->console_notebook);

    v->console_window = gtk_window_new();
    gtk_window_set_title(GTK_WINDOW(v->console_window),
                         ns_i18n("Developer Tools"));
    gtk_window_set_default_size(GTK_WINDOW(v->console_window), 720, 420);
    gtk_window_set_destroy_with_parent(GTK_WINDOW(v->console_window), TRUE);
    gtk_window_set_child(GTK_WINDOW(v->console_window), box);
    g_signal_connect(v->console_window, "close-request",
                     G_CALLBACK(on_console_close_request), v);
}

static void
build_search_bar(NsProcView *v)
{
    GtkWidget *box = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 4);
    gtk_widget_add_css_class(box, "toolbar");

    v->search_entry = gtk_search_entry_new();
    gtk_widget_set_size_request(v->search_entry, 220, -1);
    set_accessible_label(v->search_entry, ns_i18n("Find in page"));
    g_signal_connect(v->search_entry, "search-changed",
                     G_CALLBACK(on_search_changed), v);
    g_signal_connect(v->search_entry, "activate",
                     G_CALLBACK(on_search_next), v);
    g_signal_connect(v->search_entry, "next-match",
                     G_CALLBACK(on_search_next), v);
    g_signal_connect(v->search_entry, "previous-match",
                     G_CALLBACK(on_search_prev), v);
    g_signal_connect(v->search_entry, "stop-search",
                     G_CALLBACK(on_search_stop), v);

    v->search_label = gtk_label_new("");
    gtk_widget_set_size_request(v->search_label, 56, -1);

    GtkWidget *prev = gtk_button_new_from_icon_name("go-up-symbolic");
    GtkWidget *next = gtk_button_new_from_icon_name("go-down-symbolic");
    GtkWidget *close = gtk_button_new_from_icon_name("window-close-symbolic");
    gtk_widget_set_tooltip_text(prev, ns_i18n("Previous match (Shift+Enter)"));
    gtk_widget_set_tooltip_text(next, ns_i18n("Next match (Enter)"));
    gtk_widget_set_tooltip_text(close, ns_i18n("Close (Esc)"));
    set_accessible_label(prev, ns_i18n("Previous match"));
    set_accessible_label(next, ns_i18n("Next match"));
    set_accessible_label(close, ns_i18n("Close search"));
    g_signal_connect(prev, "clicked", G_CALLBACK(on_search_prev), v);
    g_signal_connect(next, "clicked", G_CALLBACK(on_search_next), v);
    g_signal_connect(close, "clicked", G_CALLBACK(on_search_close_clicked), v);

    gtk_box_append(GTK_BOX(box), v->search_entry);
    gtk_box_append(GTK_BOX(box), v->search_label);
    gtk_box_append(GTK_BOX(box), prev);
    gtk_box_append(GTK_BOX(box), next);
    gtk_box_append(GTK_BOX(box), close);

    GtkWidget *frame = gtk_frame_new(NULL);
    gtk_widget_add_css_class(frame, "ns-findbar");
    gtk_frame_set_child(GTK_FRAME(frame), box);

    v->search_revealer = gtk_revealer_new();
    gtk_revealer_set_transition_type(GTK_REVEALER(v->search_revealer),
                                     GTK_REVEALER_TRANSITION_TYPE_SLIDE_DOWN);
    gtk_revealer_set_child(GTK_REVEALER(v->search_revealer), frame);
    gtk_widget_set_halign(v->search_revealer, GTK_ALIGN_END);
    gtk_widget_set_valign(v->search_revealer, GTK_ALIGN_START);
    gtk_widget_set_margin_top(v->search_revealer, 8);
    gtk_widget_set_margin_end(v->search_revealer, 20);
    gtk_overlay_add_overlay(GTK_OVERLAY(v->overlay), v->search_revealer);
}

static void
build_perm_bar(NsProcView *v)
{
    GtkWidget *bar = gtk_box_new(GTK_ORIENTATION_HORIZONTAL, 8);
    gtk_widget_set_margin_top(bar, 4);
    gtk_widget_set_margin_bottom(bar, 4);
    gtk_widget_set_margin_start(bar, 8);
    gtk_widget_set_margin_end(bar, 8);

    GtkWidget *icon = gtk_image_new_from_icon_name("dialog-question-symbolic");

    v->perm_label = gtk_label_new("");
    gtk_label_set_xalign(GTK_LABEL(v->perm_label), 0.0);
    gtk_label_set_ellipsize(GTK_LABEL(v->perm_label), PANGO_ELLIPSIZE_MIDDLE);
    gtk_widget_set_hexpand(v->perm_label, TRUE);

    GtkWidget *allow = gtk_button_new_with_label(ns_i18n("Allow"));
    gtk_widget_add_css_class(allow, "suggested-action");
    gtk_widget_set_tooltip_text(allow, ns_i18n("Allow and trust this site"));
    GtkWidget *deny = gtk_button_new_with_label(ns_i18n("Not now"));
    GtkWidget *close = gtk_button_new_from_icon_name("window-close-symbolic");
    gtk_button_set_has_frame(GTK_BUTTON(close), FALSE);
    set_accessible_label(close, ns_i18n("Dismiss"));
    g_signal_connect(allow, "clicked", G_CALLBACK(on_perm_allow), v);
    g_signal_connect(deny, "clicked", G_CALLBACK(on_perm_deny), v);
    g_signal_connect(close, "clicked", G_CALLBACK(on_perm_deny), v);

    gtk_box_append(GTK_BOX(bar), icon);
    gtk_box_append(GTK_BOX(bar), v->perm_label);
    gtk_box_append(GTK_BOX(bar), allow);
    gtk_box_append(GTK_BOX(bar), deny);
    gtk_box_append(GTK_BOX(bar), close);

    GtkWidget *frame = gtk_frame_new(NULL);
    gtk_frame_set_child(GTK_FRAME(frame), bar);

    v->perm_revealer = gtk_revealer_new();
    gtk_revealer_set_transition_type(GTK_REVEALER(v->perm_revealer),
                                     GTK_REVEALER_TRANSITION_TYPE_SLIDE_DOWN);
    gtk_revealer_set_child(GTK_REVEALER(v->perm_revealer), frame);
}

NsProcView *
ns_proc_view_new(void)
{
    ns_pv_check_layout();
    NsProcView *v = g_new0(NsProcView, 1);
    g_ref_count_init(&v->rc);
    g_mutex_init(&v->proc_lock);
    v->media = ns_pv_media_new();
    v->renderer_path = ns_proc_renderer_path();
    v->queue = g_async_queue_new();
    v->history = g_ptr_array_new_with_free_func(g_free);
    v->hist_index = -1;
    v->link_pending_action = ACT_HOVER;
    v->pending_record = TRUE;
    v->scale = 1.0;

    v->hadj = gtk_adjustment_new(0, 0, 1, 60, 60, 1);
    v->vadj = gtk_adjustment_new(0, 0, 1, 60, 60, 1);
    ns_pv_install_view_signals(v);

    v->layers = ns_page_layers_new();
    v->area = ns_pv_area_new(v);
    gtk_widget_set_hexpand(v->area, TRUE);
    gtk_widget_set_vexpand(v->area, TRUE);
    gtk_widget_set_focusable(v->area, TRUE);
    ns_pv_install_area_signals(v);

    GtkWidget *grid = gtk_grid_new();
    v->vscroll =
        gtk_scrollbar_new(GTK_ORIENTATION_VERTICAL, v->vadj);
    v->hscroll =
        gtk_scrollbar_new(GTK_ORIENTATION_HORIZONTAL, v->hadj);
    gtk_grid_attach(GTK_GRID(grid), v->area, 0, 0, 1, 1);
    gtk_grid_attach(GTK_GRID(grid), v->vscroll, 1, 0, 1, 1);
    gtk_grid_attach(GTK_GRID(grid), v->hscroll, 0, 1, 1, 1);
    gtk_widget_set_visible(v->vscroll, FALSE);
    gtk_widget_set_visible(v->hscroll, FALSE);

    v->overlay = gtk_overlay_new();
    gtk_overlay_set_child(GTK_OVERLAY(v->overlay), grid);
    gtk_widget_set_vexpand(v->overlay, TRUE);
    build_perm_bar(v);
    v->root = gtk_box_new(GTK_ORIENTATION_VERTICAL, 0);
    gtk_box_append(GTK_BOX(v->root), v->perm_revealer);
    gtk_box_append(GTK_BOX(v->root), v->overlay);
    gtk_widget_set_vexpand(v->root, TRUE);
    build_search_bar(v);

    ns_pv_input_install_pointer(v);

    ctx_install_actions(v);
    ns_pv_input_install_secondary(v);

    ns_pv_install_scroll(v);

    ns_pv_input_install_keys(v);

    g_signal_connect(v->area, "destroy", G_CALLBACK(on_area_destroy), v);

    v->thread = ns_pv_start_worker(v);
    return v;
}

GtkWidget *ns_proc_view_widget(NsProcView *v) { return v->root; }

void
ns_proc_view_set_notify(NsProcView *v, NsProcNotify cb, gpointer ud)
{
    v->notify = cb;
    v->notify_ud = ud;
}

void
ns_proc_view_set_private(NsProcView *v, gboolean private_mode)
{
    if (v)
        v->private_mode = private_mode;
}

gboolean
ns_proc_view_is_private(NsProcView *v)
{
    return v && v->private_mode;
}

void
ns_proc_view_layout(size_t *out, size_t count)
{
    const size_t layout[] = {
        sizeof(Req),
        G_STRUCT_OFFSET(Req, type),
        G_STRUCT_OFFSET(Req, seq),
        G_STRUCT_OFFSET(Req, url),
        G_STRUCT_OFFSET(Req, vw),
        G_STRUCT_OFFSET(Req, vh),
        G_STRUCT_OFFSET(Req, w),
        G_STRUCT_OFFSET(Req, h),
        G_STRUCT_OFFSET(Req, sx),
        G_STRUCT_OFFSET(Req, sy),
        G_STRUCT_OFFSET(Req, scale),
        G_STRUCT_OFFSET(Req, dpr),
        G_STRUCT_OFFSET(Req, raster),
        G_STRUCT_OFFSET(Req, wheel),
        G_STRUCT_OFFSET(Req, x),
        G_STRUCT_OFFSET(Req, y),
        G_STRUCT_OFFSET(Req, dx),
        G_STRUCT_OFFSET(Req, dy),
        G_STRUCT_OFFSET(Req, mods),
        G_STRUCT_OFFSET(Req, kind),
        G_STRUCT_OFFSET(Req, keycode),
        G_STRUCT_OFFSET(Req, fallback_scroll),
        G_STRUCT_OFFSET(Req, fallback_x),
        G_STRUCT_OFFSET(Req, fallback_y),
        G_STRUCT_OFFSET(Req, key),
        G_STRUCT_OFFSET(Req, code),
        G_STRUCT_OFFSET(Req, action),
        G_STRUCT_OFFSET(Req, query),
        G_STRUCT_OFFSET(Req, find_dir),
        G_STRUCT_OFFSET(Req, find_from_y),
        G_STRUCT_OFFSET(Req, find_case),
        G_STRUCT_OFFSET(Req, export_dest),
        G_STRUCT_OFFSET(Req, paths),
        G_STRUCT_OFFSET(Req, dump_tab),
        G_STRUCT_OFFSET(Req, inspect),
        G_STRUCT_OFFSET(Req, history),
        G_STRUCT_OFFSET(Req, user_activated),
        G_STRUCT_OFFSET(Req, caret_active),
        G_STRUCT_OFFSET(Req, tiles),
        G_STRUCT_OFFSET(Req, have),
        G_STRUCT_OFFSET(Req, hold),
        sizeof(Res),
        G_STRUCT_OFFSET(Res, view),
        G_STRUCT_OFFSET(Res, type),
        G_STRUCT_OFFSET(Res, seq),
        G_STRUCT_OFFSET(Res, ok),
        G_STRUCT_OFFSET(Res, pw),
        G_STRUCT_OFFSET(Res, ph),
        G_STRUCT_OFFSET(Res, title),
        G_STRUCT_OFFSET(Res, url),
        G_STRUCT_OFFSET(Res, url_pushed),
        G_STRUCT_OFFSET(Res, nav),
        G_STRUCT_OFFSET(Res, security),
        G_STRUCT_OFFSET(Res, remote_ip),
        G_STRUCT_OFFSET(Res, webgl),
        G_STRUCT_OFFSET(Res, camera),
        G_STRUCT_OFFSET(Res, download),
        G_STRUCT_OFFSET(Res, audio),
        G_STRUCT_OFFSET(Res, window_action),
        G_STRUCT_OFFSET(Res, clipboard),
        G_STRUCT_OFFSET(Res, texture),
        G_STRUCT_OFFSET(Res, texture_scale),
        G_STRUCT_OFFSET(Res, layers),
        G_STRUCT_OFFSET(Res, href),
        G_STRUCT_OFFSET(Res, cursor),
        G_STRUCT_OFFSET(Res, action),
        G_STRUCT_OFFSET(Res, kind),
        G_STRUCT_OFFSET(Res, prevented),
        G_STRUCT_OFFSET(Res, edit),
        G_STRUCT_OFFSET(Res, fallback_scroll),
        G_STRUCT_OFFSET(Res, fallback_x),
        G_STRUCT_OFFSET(Res, fallback_y),
        G_STRUCT_OFFSET(Res, animating),
        G_STRUCT_OFFSET(Res, caret_blinking),
        G_STRUCT_OFFSET(Res, wheel_snapped),
        G_STRUCT_OFFSET(Res, frame_unchanged),
        G_STRUCT_OFFSET(Res, requested_scroll_y),
        G_STRUCT_OFFSET(Res, requested_scroll_x),
        G_STRUCT_OFFSET(Res, find_total),
        G_STRUCT_OFFSET(Res, find_current),
        G_STRUCT_OFFSET(Res, find_scroll_y),
        G_STRUCT_OFFSET(Res, media_url),
        G_STRUCT_OFFSET(Res, media_is_video),
        G_STRUCT_OFFSET(Res, media_stream),
        G_STRUCT_OFFSET(Res, favicon_data),
        G_STRUCT_OFFSET(Res, favicon_w),
        G_STRUCT_OFFSET(Res, favicon_h),
        G_STRUCT_OFFSET(Res, favicon_stride),
        G_STRUCT_OFFSET(Res, dump_tab),
        G_STRUCT_OFFSET(Res, inspect),
        G_STRUCT_OFFSET(Res, print_pages),
        G_STRUCT_OFFSET(Res, print_setup),
        G_STRUCT_OFFSET(Res, print_scale),
        sizeof(NsProcView),
        G_STRUCT_OFFSET(NsProcView, rc),
        G_STRUCT_OFFSET(NsProcView, root),
        G_STRUCT_OFFSET(NsProcView, area),
        G_STRUCT_OFFSET(NsProcView, hscroll),
        G_STRUCT_OFFSET(NsProcView, vscroll),
        G_STRUCT_OFFSET(NsProcView, im),
        G_STRUCT_OFFSET(NsProcView, hadj),
        G_STRUCT_OFFSET(NsProcView, vadj),
        G_STRUCT_OFFSET(NsProcView, closed),
        G_STRUCT_OFFSET(NsProcView, thread),
        G_STRUCT_OFFSET(NsProcView, queue),
        G_STRUCT_OFFSET(NsProcView, proc),
        G_STRUCT_OFFSET(NsProcView, proc_lock),
        G_STRUCT_OFFSET(NsProcView, renderer_path),
        G_STRUCT_OFFSET(NsProcView, private_mode),
        G_STRUCT_OFFSET(NsProcView, media),
        G_STRUCT_OFFSET(NsProcView, notify),
        G_STRUCT_OFFSET(NsProcView, notify_ud),
        G_STRUCT_OFFSET(NsProcView, current_url),
        G_STRUCT_OFFSET(NsProcView, current_title),
        G_STRUCT_OFFSET(NsProcView, security),
        G_STRUCT_OFFSET(NsProcView, remote_ip),
        G_STRUCT_OFFSET(NsProcView, page_w),
        G_STRUCT_OFFSET(NsProcView, page_h),
        G_STRUCT_OFFSET(NsProcView, scroll_x),
        G_STRUCT_OFFSET(NsProcView, scroll_y),
        G_STRUCT_OFFSET(NsProcView, opened),
        G_STRUCT_OFFSET(NsProcView, frame),
        G_STRUCT_OFFSET(NsProcView, frame_scale),
        G_STRUCT_OFFSET(NsProcView, layers),
        G_STRUCT_OFFSET(NsProcView, tiles_mode),
        G_STRUCT_OFFSET(NsProcView, tiles_fill),
        G_STRUCT_OFFSET(NsProcView, wheel_remote),
        G_STRUCT_OFFSET(NsProcView, scroll_dir),
        G_STRUCT_OFFSET(NsProcView, surface_scale_handler),
        G_STRUCT_OFFSET(NsProcView, scale_surface),
        G_STRUCT_OFFSET(NsProcView, stage),
        G_STRUCT_OFFSET(NsProcView, stage_next),
        G_STRUCT_OFFSET(NsProcView, favicon),
        G_STRUCT_OFFSET(NsProcView, render_inflight),
        G_STRUCT_OFFSET(NsProcView, render_pending),
        G_STRUCT_OFFSET(NsProcView, tick_inflight),
        G_STRUCT_OFFSET(NsProcView, tick_pending),
        G_STRUCT_OFFSET(NsProcView, render_restarts),
        G_STRUCT_OFFSET(NsProcView, link_inflight),
        G_STRUCT_OFFSET(NsProcView, link_pending),
        G_STRUCT_OFFSET(NsProcView, link_pending_x),
        G_STRUCT_OFFSET(NsProcView, link_pending_y),
        G_STRUCT_OFFSET(NsProcView, link_pending_action),
        G_STRUCT_OFFSET(NsProcView, hover_inflight),
        G_STRUCT_OFFSET(NsProcView, hover_pending),
        G_STRUCT_OFFSET(NsProcView, hover_pending_x),
        G_STRUCT_OFFSET(NsProcView, hover_pending_y),
        G_STRUCT_OFFSET(NsProcView, has_selection),
        G_STRUCT_OFFSET(NsProcView, multi_click),
        G_STRUCT_OFFSET(NsProcView, ctx_x),
        G_STRUCT_OFFSET(NsProcView, ctx_y),
        G_STRUCT_OFFSET(NsProcView, ctx_link),
        G_STRUCT_OFFSET(NsProcView, ctx_popover),
        G_STRUCT_OFFSET(NsProcView, ctx_actions),
        G_STRUCT_OFFSET(NsProcView, search_revealer),
        G_STRUCT_OFFSET(NsProcView, search_entry),
        G_STRUCT_OFFSET(NsProcView, search_label),
        G_STRUCT_OFFSET(NsProcView, find_seq),
        G_STRUCT_OFFSET(NsProcView, find_case),
        G_STRUCT_OFFSET(NsProcView, overlay),
        G_STRUCT_OFFSET(NsProcView, perm_revealer),
        G_STRUCT_OFFSET(NsProcView, perm_label),
        G_STRUCT_OFFSET(NsProcView, perm_kind),
        G_STRUCT_OFFSET(NsProcView, perm_pending),
        G_STRUCT_OFFSET(NsProcView, perm_origin),
        G_STRUCT_OFFSET(NsProcView, console_window),
        G_STRUCT_OFFSET(NsProcView, console_notebook),
        G_STRUCT_OFFSET(NsProcView, console_entry),
        G_STRUCT_OFFSET(NsProcView, console_view),
        G_STRUCT_OFFSET(NsProcView, console_buffer),
        G_STRUCT_OFFSET(NsProcView, net_view),
        G_STRUCT_OFFSET(NsProcView, net_buffer),
        G_STRUCT_OFFSET(NsProcView, perf_view),
        G_STRUCT_OFFSET(NsProcView, perf_buffer),
        G_STRUCT_OFFSET(NsProcView, layout_view),
        G_STRUCT_OFFSET(NsProcView, layout_buffer),
        G_STRUCT_OFFSET(NsProcView, elements_view),
        G_STRUCT_OFFSET(NsProcView, elements_buffer),
        G_STRUCT_OFFSET(NsProcView, inspect_entry),
        G_STRUCT_OFFSET(NsProcView, console_open),
        G_STRUCT_OFFSET(NsProcView, console_poll_id),
        G_STRUCT_OFFSET(NsProcView, history),
        G_STRUCT_OFFSET(NsProcView, hist_index),
        G_STRUCT_OFFSET(NsProcView, pending_record),
        G_STRUCT_OFFSET(NsProcView, deferred_url),
        G_STRUCT_OFFSET(NsProcView, deferred_record),
        G_STRUCT_OFFSET(NsProcView, deferred_history),
        G_STRUCT_OFFSET(NsProcView, deferred_user_activated),
        G_STRUCT_OFFSET(NsProcView, js_redirects),
        G_STRUCT_OFFSET(NsProcView, scale),
        G_STRUCT_OFFSET(NsProcView, loading),
        G_STRUCT_OFFSET(NsProcView, busy_cursor),
        G_STRUCT_OFFSET(NsProcView, hourglass_cursor),
        G_STRUCT_OFFSET(NsProcView, load_seq),
        G_STRUCT_OFFSET(NsProcView, render_seq),
        G_STRUCT_OFFSET(NsProcView, link_seq),
        G_STRUCT_OFFSET(NsProcView, click_seq),
        G_STRUCT_OFFSET(NsProcView, viewport_seq),
        G_STRUCT_OFFSET(NsProcView, key_seq),
        G_STRUCT_OFFSET(NsProcView, select_seq),
        G_STRUCT_OFFSET(NsProcView, hover_seq),
        G_STRUCT_OFFSET(NsProcView, last_vp_w),
        G_STRUCT_OFFSET(NsProcView, last_vp_h),
        G_STRUCT_OFFSET(NsProcView, last_vp_dpr),
        G_STRUCT_OFFSET(NsProcView, wheel_left_x),
        G_STRUCT_OFFSET(NsProcView, wheel_left_y),
        G_STRUCT_OFFSET(NsProcView, wheel_viewport),
        G_STRUCT_OFFSET(NsProcView, wheel_pend_x),
        G_STRUCT_OFFSET(NsProcView, wheel_pend_y),
        G_STRUCT_OFFSET(NsProcView, fling_vx),
        G_STRUCT_OFFSET(NsProcView, fling_vy),
        G_STRUCT_OFFSET(NsProcView, wheel_tick_id),
        G_STRUCT_OFFSET(NsProcView, wheel_last_us),
        G_STRUCT_OFFSET(NsProcView, adopting_scroll),
        G_STRUCT_OFFSET(NsProcView, hover_after_scroll_id),
        G_STRUCT_OFFSET(NsProcView, drag_start_x),
        G_STRUCT_OFFSET(NsProcView, drag_start_y),
        G_STRUCT_OFFSET(NsProcView, pointer_x),
        G_STRUCT_OFFSET(NsProcView, pointer_y),
        G_STRUCT_OFFSET(NsProcView, drag_anchored),
        G_STRUCT_OFFSET(NsProcView, sb_probe),
        G_STRUCT_OFFSET(NsProcView, sb_dragging),
        G_STRUCT_OFFSET(NsProcView, sb_have_last),
        G_STRUCT_OFFSET(NsProcView, sb_last_x),
        G_STRUCT_OFFSET(NsProcView, sb_last_y),
        G_STRUCT_OFFSET(NsProcView, anim_tick_id),
        G_STRUCT_OFFSET(NsProcView, last_anim_frame_us),
        G_STRUCT_OFFSET(NsProcView, page_animating),
        G_STRUCT_OFFSET(NsProcView, caret_blinking),
    };
    for (size_t i = 0; i < count && i < G_N_ELEMENTS(layout); i++)
        out[i] = layout[i];
}
