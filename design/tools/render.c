/* Renders one GtkBuilder mockup window to a PNG at 2x.
 *
 * No GTK headers are installed on this machine, so the few functions used are
 * declared by hand and linked against the runtime libraries.
 *
 * usage: render UI OUT.png WIDTH HEIGHT [--light] [--page NAME]
 *                                       [--toast TEXT] [--toast-button LABEL]
 *                                       [--dialog-ui UI]
 *
 * Builder objects with special meaning: "window" (required), "dialog" (an
 * AdwDialog presented over the window, from UI or from --dialog-ui),
 * "toasts" (an AdwToastOverlay).
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef void *gp;
typedef struct { float x, y; } point_t;
typedef struct { float x, y, w, h; } rect_t;

extern void adw_init(void);
extern gp g_settings_new(const char *);
extern gp gtk_settings_get_default(void);
extern void g_object_set(gp, const char *, ...);
extern int g_settings_set_string(gp, const char *, const char *);
extern gp gtk_builder_new_from_file(const char *);
extern gp gtk_builder_get_object(gp, const char *);
extern void gtk_window_set_default_size(gp, int, int);
extern void gtk_window_present(gp);
extern void adw_dialog_present(gp, gp);
extern void adw_preferences_dialog_set_visible_page_name(gp, const char *);
extern gp adw_style_manager_get_default(void);
extern void adw_style_manager_set_color_scheme(gp, int);
extern gp adw_toast_new(const char *);
extern void adw_toast_set_timeout(gp, unsigned);
extern void adw_toast_set_button_label(gp, const char *);
extern void adw_toast_overlay_add_toast(gp, gp);
extern gp gtk_css_provider_new(void);
extern void gtk_css_provider_load_from_path(gp, const char *);
extern gp gdk_display_get_default(void);
extern gp gtk_icon_theme_get_for_display(gp);
extern void gtk_icon_theme_add_search_path(gp, const char *);
extern void gtk_style_context_add_provider_for_display(gp, gp, unsigned);
extern unsigned g_timeout_add(unsigned, int (*)(gp), gp);
extern gp g_main_loop_new(gp, int);
extern void g_main_loop_run(gp);
extern void g_main_loop_quit(gp);
extern gp gtk_widget_paintable_new(gp);
extern gp gtk_snapshot_new(void);
extern void gtk_snapshot_translate(gp, const point_t *);
extern void gtk_snapshot_scale(gp, float, float);
extern void gdk_paintable_snapshot(gp, gp, double, double);
extern gp gtk_snapshot_free_to_node(gp);
extern gp gtk_native_get_renderer(gp);
extern gp gsk_renderer_render_texture(gp, gp, const rect_t *);
extern int gdk_texture_save_to_png(gp, const char *);
extern int gtk_widget_get_width(gp);
extern int gtk_widget_get_height(gp);

#define MARGIN 48.0f /* room for the window shadow */

static gp loop, window;
static float SCALE = 2.0f; /* RENDER_SCALE overrides */
static const char *out;

static int shoot(gp unused)
{
    (void)unused;
    int w = gtk_widget_get_width(window), h = gtk_widget_get_height(window);
    gp paintable = gtk_widget_paintable_new(window);
    gp snap = gtk_snapshot_new();
    gtk_snapshot_scale(snap, SCALE, SCALE);
    point_t off = { MARGIN, MARGIN };
    gtk_snapshot_translate(snap, &off);
    gdk_paintable_snapshot(paintable, snap, w, h);
    gp node = gtk_snapshot_free_to_node(snap);
    rect_t view = { 0, 0, (w + 2 * MARGIN) * SCALE, (h + 2 * MARGIN) * SCALE };
    gp tex = gsk_renderer_render_texture(gtk_native_get_renderer(window), node, &view);
    if (!gdk_texture_save_to_png(tex, out)) {
        fprintf(stderr, "could not save %s\n", out);
        exit(1);
    }
    printf("%s %dx%d\n", out, w, h);
    g_main_loop_quit(loop);
    return 0;
}

int main(int argc, char **argv)
{
    if (argc < 5) {
        fprintf(stderr, "usage: render UI OUT.png WIDTH HEIGHT [options]\n");
        return 2;
    }
    const char *page = NULL, *toast_text = NULL, *toast_button = NULL, *dialog_ui = NULL;
    int light = 0;
    for (int i = 5; i < argc; i++) {
        if (!strcmp(argv[i], "--light")) light = 1;
        else if (!strcmp(argv[i], "--page") && i + 1 < argc) page = argv[++i];
        else if (!strcmp(argv[i], "--toast") && i + 1 < argc) toast_text = argv[++i];
        else if (!strcmp(argv[i], "--toast-button") && i + 1 < argc) toast_button = argv[++i];
        else if (!strcmp(argv[i], "--dialog-ui") && i + 1 < argc) dialog_ui = argv[++i];
    }
    out = argv[2];
    if (getenv("RENDER_SCALE")) SCALE = atof(getenv("RENDER_SCALE"));

    /* Stock GNOME look, whatever this machine's GSettings defaults are
     * (needs GSETTINGS_BACKEND=memory so nothing is written). */
    if (getenv("MOCKUP_STOCK")) {
        g_settings_set_string(g_settings_new("org.gnome.desktop.interface"), "accent-color", "blue");
        g_settings_set_string(g_settings_new("org.gnome.desktop.wm.preferences"), "button-layout", "appmenu:close");
    }
    adw_init();
    if (getenv("MOCKUP_STOCK"))
        g_object_set(gtk_settings_get_default(), "gtk-decoration-layout", "appmenu:close", NULL);
    /* ADW_COLOR_SCHEME_FORCE_LIGHT = 1, ADW_COLOR_SCHEME_FORCE_DARK = 4 */
    adw_style_manager_set_color_scheme(adw_style_manager_get_default(), light ? 1 : 4);

    const char *icons = getenv("MOCKUP_ICONS");
    if (icons)
        gtk_icon_theme_add_search_path(gtk_icon_theme_get_for_display(gdk_display_get_default()), icons);

    const char *css = getenv("MOCKUP_CSS");
    if (css) {
        gp provider = gtk_css_provider_new();
        gtk_css_provider_load_from_path(provider, css);
        gtk_style_context_add_provider_for_display(gdk_display_get_default(), provider, 600);
    }

    gp builder = gtk_builder_new_from_file(argv[1]);
    window = gtk_builder_get_object(builder, "window");
    if (!window) {
        fprintf(stderr, "%s has no object with id \"window\"\n", argv[1]);
        return 1;
    }
    gtk_window_set_default_size(window, atoi(argv[3]), atoi(argv[4]));
    gtk_window_present(window);

    gp dialog = gtk_builder_get_object(dialog_ui ? gtk_builder_new_from_file(dialog_ui) : builder, "dialog");
    if (dialog) {
        if (page) adw_preferences_dialog_set_visible_page_name(dialog, page);
        adw_dialog_present(dialog, window);
    }
    gp toasts = gtk_builder_get_object(builder, "toasts");
    if (toasts && toast_text) {
        gp toast = adw_toast_new(toast_text);
        adw_toast_set_timeout(toast, 0);
        if (toast_button) adw_toast_set_button_label(toast, toast_button);
        adw_toast_overlay_add_toast(toasts, toast);
    }

    loop = g_main_loop_new(NULL, 0);
    g_timeout_add(1800, shoot, NULL);
    g_main_loop_run(loop);
    return 0;
}
