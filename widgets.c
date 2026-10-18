#define _POSIX_C_SOURCE 200809L
#include <gtk/gtk.h>
#include <gtk-layer-shell.h>
#include <glib-unix.h>
#include <pango/pangocairo.h>
#include <sys/file.h>
#include <sys/statvfs.h>
#include <fcntl.h>
#include <unistd.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include "metrics.h"
#include "layout.h"

#define WIDTH 300
#define HEIGHT 628
static const char *months[] = {"ЯНВАРЬ","ФЕВРАЛЬ","МАРТ","АПРЕЛЬ","МАЙ","ИЮНЬ","ИЮЛЬ","АВГУСТ","СЕНТЯБРЬ","ОКТЯБРЬ","НОЯБРЬ","ДЕКАБРЬ"};
static const char *weekdays[] = {"ВОСКРЕСЕНЬ","ПОНЕДЕЛЬНИК","ВТОРНИК","СРЕДА","ЧЕТВЕРГ","ПЯТНИЦА","СУББОТА"};
static GtkWidget *windows[PANEL_COUNT], *grid_window;
static GdkMonitor *monitor;
static PanelLayout layout;
static int monitor_width,monitor_height,drag_index=-1;
static PanelPosition drag_initial,drag_preview;
static double drag_start_x,drag_start_y;
static char *layout_path;
static char *state_dir;
static CpuSample previous;
static bool have_cpu, cpu_ready, memory_ready, disk_ready;
static double cpu, ram_used, ram_total, disk_free, disk_used_fraction;
static unsigned ticks;
static struct tm now;
static const unsigned FG=0xd4be98, MUTED=0xa89984, ACCENT=0xe78a4e, GREEN=0xa9b665, BORDER=0x504945;

static void color(cairo_t *cr, unsigned hex, double alpha) {
    cairo_set_source_rgba(cr, ((hex>>16)&255)/255., ((hex>>8)&255)/255., (hex&255)/255., alpha);
}
static void box(cairo_t *cr, double x, double y, double w, double h, unsigned hex, double alpha) {
    color(cr,hex,alpha); cairo_rectangle(cr,x,y,w,h); cairo_fill(cr);
}
static void text(cairo_t *cr, const char *s, double x, double y, int size, unsigned hex, int width, PangoAlignment align) {
    PangoLayout *layout=pango_cairo_create_layout(cr);
    PangoFontDescription *font=pango_font_description_new();
    pango_font_description_set_family(font,"MxPlus IBM VGA 8x16");
    pango_font_description_set_absolute_size(font,size*PANGO_SCALE);
    pango_layout_set_font_description(layout,font);
    pango_layout_set_text(layout,s,-1);
    if (width>0) { pango_layout_set_width(layout,width*PANGO_SCALE); pango_layout_set_alignment(layout,align); }
    color(cr,hex,1); cairo_move_to(cr,x,y); pango_cairo_show_layout(cr,layout);
    pango_font_description_free(font); g_object_unref(layout);
}
static void card(cairo_t *cr, int y, int h) {
    box(cr,0,y,WIDTH,h,0x282828,.96);
    color(cr,BORDER,1); cairo_set_line_width(cr,1); cairo_rectangle(cr,.5,y+.5,WIDTH-1,h-1); cairo_stroke(cr);
    box(cr,0,y+18,2,16,ACCENT,1);
    for(int row=0;row<3;row++) box(cr,268,y+22+row*4,10,1,MUTED,.7);
}
static void bar(cairo_t *cr, int y, double fraction, unsigned hex) {
    box(cr,24,y,252,3,0x3c3836,1);
    box(cr,24,y,252*CLAMP(fraction,0.,1.),3,hex,1);
}
static void sample(void) {
    time_t timestamp=time(NULL); localtime_r(&timestamp,&now);
    char *contents=NULL;
    if (g_file_get_contents("/proc/stat",&contents,NULL,NULL)) {
        CpuSample current;
        if (parse_cpu(contents,&current)) {
            if (have_cpu) { cpu=cpu_percent(previous,current); cpu_ready=true; }
            previous=current; have_cpu=true;
        } else cpu_ready=false;
        g_free(contents);
    } else cpu_ready=false;
    contents=NULL;
    memory_ready=g_file_get_contents("/proc/meminfo",&contents,NULL,NULL) && parse_memory(contents,&ram_used,&ram_total);
    g_free(contents);
    struct statvfs fs;
    disk_ready=statvfs(g_get_home_dir(),&fs)==0 && fs.f_blocks>0;
    if (disk_ready) {
        disk_free=(double)fs.f_bavail*fs.f_frsize/1073741824.;
        /* df-style usage excludes reserved blocks from available capacity. */
        double used=fs.f_blocks-fs.f_bfree;
        disk_used_fraction=(used+fs.f_bavail)>0 ? used/(used+fs.f_bavail) : 0;
    }
}
static void render_panel(cairo_t *cr,int index) {
    static const int offsets[]={0,160,450};
    char s[128];
    cairo_save(cr); cairo_translate(cr,0,-offsets[index]);
    card(cr,offsets[index],panel_heights[index]);
    if(index==0) {
        text(cr,"СЕЙЧАС",24,20,16,MUTED,0,PANGO_ALIGN_LEFT);
        strftime(s,sizeof s,"%H:%M",&now);
        text(cr,s,24,42,64,FG,252,PANGO_ALIGN_CENTER);
        snprintf(s,sizeof s,"%s · %02d.%02d",weekdays[now.tm_wday],now.tm_mday,now.tm_mon+1);
        text(cr,s,24,113,16,ACCENT,252,PANGO_ALIGN_CENTER);
    } else if(index==1) {
        text(cr,"КАЛЕНДАРЬ",24,180,16,MUTED,0,PANGO_ALIGN_LEFT);
        snprintf(s,sizeof s,"%s %d",months[now.tm_mon],now.tm_year+1900);
        text(cr,s,24,208,16,FG,252,PANGO_ALIGN_CENTER);
        static const char *days[]={"ПН","ВТ","СР","ЧТ","ПТ","СБ","ВС"};
        for (int c=0;c<7;c++) text(cr,days[c],24+c*36,244,16,c>=5?ACCENT:MUTED,36,PANGO_ALIGN_CENTER);
        int first=month_start(now.tm_year+1900,now.tm_mon+1);
        for (int day=1;day<=month_days(now.tm_year+1900,now.tm_mon+1);day++) {
            int cell=first+day-1, col=cell%7, row=cell/7, x=24+col*36, y=272+row*26;
            bool today=day==now.tm_mday;
            if (today) box(cr,x+4,y-3,28,22,ACCENT,1);
            snprintf(s,sizeof s,"%2d",day);
            text(cr,s,x,y,16,today?0x282828:(col>=5?ACCENT:FG),36,PANGO_ALIGN_CENTER);
        }
    } else {
        text(cr,"СИСТЕМА",24,470,16,MUTED,0,PANGO_ALIGN_LEFT);
        text(cr,"CPU",24,500,16,FG,0,PANGO_ALIGN_LEFT);
        if (cpu_ready) snprintf(s,sizeof s,"%.0f%%",cpu); else g_strlcpy(s,"—",sizeof s);
        text(cr,s,72,500,16,ACCENT,204,PANGO_ALIGN_RIGHT); bar(cr,524,cpu_ready?cpu/100.:0,ACCENT);
        text(cr,"ОЗУ",24,540,16,FG,0,PANGO_ALIGN_LEFT);
        if (memory_ready) snprintf(s,sizeof s,"%.1f / %.1f GiB",ram_used,ram_total); else g_strlcpy(s,"—",sizeof s);
        text(cr,s,72,540,16,FG,204,PANGO_ALIGN_RIGHT); bar(cr,564,memory_ready?ram_used/ram_total:0,GREEN);
        text(cr,"ДИСК",24,580,16,FG,0,PANGO_ALIGN_LEFT);
        if (disk_ready) snprintf(s,sizeof s,"%.0f GiB своб.",disk_free); else g_strlcpy(s,"—",sizeof s);
        text(cr,s,72,580,16,FG,204,PANGO_ALIGN_RIGHT); bar(cr,604,disk_ready?disk_used_fraction:0,MUTED);
    }
    cairo_restore(cr);
}
static void clear_surface(cairo_t *cr) {
    cairo_set_operator(cr,CAIRO_OPERATOR_SOURCE);
    cairo_set_source_rgba(cr,0,0,0,0); cairo_paint(cr);
    cairo_set_operator(cr,CAIRO_OPERATOR_OVER);
}
static gboolean draw(GtkWidget *widget,cairo_t *cr,gpointer data) {
    (void)widget;
    int index=GPOINTER_TO_INT(data);
    clear_surface(cr);
    if(index!=drag_index) render_panel(cr,index);
    return FALSE;
}
static void input_shape(GtkWidget *widget,gpointer data) {
    int index=GPOINTER_TO_INT(data);
    cairo_region_t *region=cairo_region_create();
    if(index>=0) {
        cairo_rectangle_int_t header={0,0,PANEL_WIDTH,42};
        cairo_region_union_rectangle(region,&header);
        GdkCursor *cursor=gdk_cursor_new_from_name(gtk_widget_get_display(widget),"grab");
        gdk_window_set_cursor(gtk_widget_get_window(widget),cursor);
        if(cursor) g_object_unref(cursor);
    }
    gdk_window_input_shape_combine_region(gtk_widget_get_window(widget),region,0,0);
    cairo_region_destroy(region);
}
static void render_grid(cairo_t *cr,int width,int height) {
    box(cr,0,0,width,height,0x1b1b1b,.08);
    cairo_set_line_width(cr,1);
    for(int major=0;major<2;major++) {
        color(cr,ACCENT,major?.32:.13);
        for(int x=0;x<width;x+=GRID_SIZE) {
            if((x%(GRID_SIZE*4)==0)!=major) continue;
            cairo_move_to(cr,x+.5,0); cairo_line_to(cr,x+.5,height);
        }
        for(int y=0;y<height;y+=GRID_SIZE) {
            if((y%(GRID_SIZE*4)==0)!=major) continue;
            cairo_move_to(cr,0,y+.5); cairo_line_to(cr,width,y+.5);
        }
        cairo_stroke(cr);
    }
}
static gboolean draw_grid(GtkWidget *widget,cairo_t *cr,gpointer data) {
    (void)data;
    clear_surface(cr);
    render_grid(cr,gtk_widget_get_allocated_width(widget),gtk_widget_get_allocated_height(widget));
    if(drag_index>=0) {
        cairo_save(cr); cairo_translate(cr,drag_preview.x,drag_preview.y);
        render_panel(cr,drag_index);
        color(cr,ACCENT,1); cairo_set_line_width(cr,2);
        cairo_rectangle(cr,1,1,PANEL_WIDTH-2,panel_heights[drag_index]-2); cairo_stroke(cr);
        cairo_restore(cr);
    }
    return FALSE;
}
static GtkWidget *make_layer(GtkLayerShellLayer layer,int width,int height) {
    GtkWidget *widget=gtk_window_new(GTK_WINDOW_TOPLEVEL);
    gtk_window_set_title(GTK_WINDOW(widget),"Desktop Widgets");
    gtk_window_set_decorated(GTK_WINDOW(widget),FALSE);
    gtk_window_set_accept_focus(GTK_WINDOW(widget),FALSE);
    gtk_widget_set_app_paintable(widget,TRUE);
    GdkVisual *visual=gdk_screen_get_rgba_visual(gtk_widget_get_screen(widget));
    if(visual) gtk_widget_set_visual(widget,visual);
    gtk_layer_init_for_window(GTK_WINDOW(widget));
    gtk_layer_set_namespace(GTK_WINDOW(widget),"desktop-widgets");
    gtk_layer_set_monitor(GTK_WINDOW(widget),monitor);
    gtk_layer_set_layer(GTK_WINDOW(widget),layer);
    gtk_layer_set_anchor(GTK_WINDOW(widget),GTK_LAYER_SHELL_EDGE_TOP,TRUE);
    gtk_layer_set_anchor(GTK_WINDOW(widget),GTK_LAYER_SHELL_EDGE_LEFT,TRUE);
    gtk_layer_set_exclusive_zone(GTK_WINDOW(widget),-1);
    gtk_layer_set_keyboard_mode(GTK_WINDOW(widget),GTK_LAYER_SHELL_KEYBOARD_MODE_NONE);
    gtk_widget_set_size_request(widget,width,height);
    return widget;
}
static void place_panel(int index) {
    gtk_layer_set_margin(GTK_WINDOW(windows[index]),GTK_LAYER_SHELL_EDGE_LEFT,layout.pos[index].x);
    gtk_layer_set_margin(GTK_WINDOW(windows[index]),GTK_LAYER_SHELL_EDGE_TOP,layout.pos[index].y);
    gtk_widget_queue_draw(windows[index]);
}
static void finish_drag(gboolean commit) {
    if(drag_index<0) return;
    int index=drag_index;
    drag_index=-1;
    if(commit) {
        layout.pos[index]=drag_preview;
        GError *error=NULL;
        if(!layout_save(&layout,layout_path,&error)) {
            g_warning("Cannot save widget positions: %s",error->message); g_error_free(error);
        }
    }
    place_panel(index);
    if(grid_window) { gtk_widget_destroy(grid_window); grid_window=NULL; }
}
static gboolean press(GtkWidget *widget,GdkEventButton *event,gpointer data) {
    (void)widget;
    if(event->button!=1 || event->type!=GDK_BUTTON_PRESS || drag_index>=0) return FALSE;
    drag_index=GPOINTER_TO_INT(data);
    drag_initial=drag_preview=layout.pos[drag_index];
    drag_start_x=event->x; drag_start_y=event->y;
    grid_window=make_layer(GTK_LAYER_SHELL_LAYER_TOP,monitor_width,monitor_height);
    g_signal_connect(grid_window,"draw",G_CALLBACK(draw_grid),NULL);
    g_signal_connect(grid_window,"realize",G_CALLBACK(input_shape),GINT_TO_POINTER(-1));
    gtk_widget_show_all(grid_window);
    gtk_widget_queue_draw(windows[drag_index]);
    return TRUE;
}
static void track_drag(double x,double y) {
    drag_preview=layout_drag(drag_initial,drag_start_x,drag_start_y,x,y,
                             monitor_width,monitor_height,panel_heights[drag_index]);
    gtk_widget_queue_draw(grid_window);
}
static gboolean motion(GtkWidget *widget,GdkEventMotion *event,gpointer data) {
    (void)widget;
    if(drag_index!=GPOINTER_TO_INT(data)) return FALSE;
    if(!(event->state&GDK_BUTTON1_MASK)) { finish_drag(FALSE); return TRUE; }
    track_drag(event->x,event->y); return TRUE;
}
static gboolean release(GtkWidget *widget,GdkEventButton *event,gpointer data) {
    (void)widget;
    if(event->button!=1 || drag_index!=GPOINTER_TO_INT(data)) return FALSE;
    track_drag(event->x,event->y); finish_drag(TRUE); return TRUE;
}
static gboolean grab_broken(GtkWidget *widget,GdkEventGrabBroken *event,gpointer data) {
    (void)widget; (void)event;
    if(drag_index==GPOINTER_TO_INT(data)) finish_drag(FALSE);
    return FALSE;
}
static void geometry_changed(GObject *object,GParamSpec *property,gpointer data) {
    (void)object; (void)property; (void)data;
    finish_drag(FALSE);
    GdkRectangle geometry; gdk_monitor_get_geometry(monitor,&geometry);
    monitor_width=geometry.width; monitor_height=geometry.height;
    layout_constrain(&layout,monitor_width,monitor_height);
    for(int i=0;i<PANEL_COUNT;i++) if(windows[i]) place_panel(i);
}
static void monitor_removed(GdkDisplay *display,GdkMonitor *removed,gpointer data) {
    (void)data;
    if(removed!=monitor) return;
    finish_drag(FALSE);
    g_signal_handlers_disconnect_by_func(monitor,geometry_changed,NULL);
    g_clear_object(&monitor);
    if(gdk_display_get_n_monitors(display)==0) { gtk_main_quit(); return; }
    monitor=g_object_ref(gdk_display_get_monitor(display,0));
    g_signal_connect(monitor,"notify::geometry",G_CALLBACK(geometry_changed),NULL);
    for(int i=0;i<PANEL_COUNT;i++) gtk_layer_set_monitor(GTK_WINDOW(windows[i]),monitor);
    geometry_changed(NULL,NULL,NULL);
}
static void write_status(void) {
    char *contents=NULL;
    long rss=0, pss=0;
    if (g_file_get_contents("/proc/self/smaps_rollup",&contents,NULL,NULL)) {
        const char *r=strstr(contents,"\nRss:"), *p=strstr(contents,"\nPss:");
        if(r) sscanf(r,"\nRss: %ld",&rss);
        if(p) sscanf(p,"\nPss: %ld",&pss);
        g_free(contents);
    }
    char *json=g_strdup_printf("{\"pid\":%ld,\"updated\":%ld,\"rss_kib\":%ld,\"pss_kib\":%ld,\"interval_seconds\":5,\"version\":2}\n",(long)getpid(),(long)time(NULL),rss,pss);
    char *path=g_build_filename(state_dir,"status.json",NULL);
    g_file_set_contents(path,json,-1,NULL);
    g_free(path); g_free(json);
}
static gboolean update(gpointer data) {
    (void)data; sample();
    for(int i=0;i<PANEL_COUNT;i++) gtk_widget_queue_draw(windows[i]);
    if(grid_window) gtk_widget_queue_draw(grid_window);
    ++ticks;
    if(ticks==1 || ticks%12==0) write_status();
    return G_SOURCE_CONTINUE;
}
static gboolean quit(gpointer data) { (void)data; gtk_main_quit(); return G_SOURCE_REMOVE; }
static int preview(const char *path, const char *wallpaper,gboolean grid) {
    int w=grid?1920:WIDTH,h=grid?1080:HEIGHT;
    GdkPixbuf *image=NULL;
    if(wallpaper) {
        GError *error=NULL;
        image=gdk_pixbuf_new_from_file(wallpaper,&error);
        if (!image) { fprintf(stderr,"%s\n",error->message); g_error_free(error); return 1; }
        w=gdk_pixbuf_get_width(image); h=gdk_pixbuf_get_height(image);
    }
    cairo_surface_t *surface=cairo_image_surface_create(CAIRO_FORMAT_ARGB32,w,h);
    cairo_t *cr=cairo_create(surface);
    if(image) {
        gdk_cairo_set_source_pixbuf(cr,image,0,0); cairo_paint(cr);
        cairo_translate(cr,w-WIDTH-40,64); g_object_unref(image);
    }
    sample();
    static const int offsets[]={0,160,450};
    PanelLayout demo;
    if(grid) {
        layout_defaults(&demo,w,h);
        demo.pos[1]=layout_drag(demo.pos[1],0,0,-480,-32,w,h,panel_heights[1]);
        render_grid(cr,w,h);
    }
    for(int i=0;i<PANEL_COUNT;i++) {
        cairo_save(cr);
        cairo_translate(cr,grid?demo.pos[i].x:0,grid?demo.pos[i].y:offsets[i]);
        render_panel(cr,i); cairo_restore(cr);
    }
    cairo_status_t result=cairo_surface_write_to_png(surface,path);
    cairo_destroy(cr); cairo_surface_destroy(surface);
    if(result!=CAIRO_STATUS_SUCCESS) { fprintf(stderr,"%s\n",cairo_status_to_string(result)); return 1; }
    return 0;
}
int main(int argc, char **argv) {
    if(argc>=3 && strcmp(argv[1],"--preview")==0) return preview(argv[2],argc>3?argv[3]:NULL,FALSE);
    if(argc==3 && strcmp(argv[1],"--preview-grid")==0) return preview(argv[2],NULL,TRUE);
    if(argc>1) { fprintf(stderr,"Usage: desktop-widgets [--preview output.png [wallpaper.jpg] | --preview-grid output.png]\n"); return 2; }
    g_setenv("GDK_BACKEND","wayland",TRUE);
    g_setenv("NO_AT_BRIDGE","1",TRUE);
    if(!gtk_init_check(&argc,&argv)) { fprintf(stderr,"Cannot connect to Wayland. Launch from your desktop session.\n"); return 1; }
    if(!gtk_layer_is_supported()) { fprintf(stderr,"Compositor does not support layer-shell.\n"); return 1; }
    state_dir=g_build_filename(g_get_user_state_dir(),"desktop-widgets",NULL);
    if(g_mkdir_with_parents(state_dir,0700)!=0) { perror("state directory"); return 1; }
    char *lock_path=g_build_filename(state_dir,"lock",NULL);
    int lock_fd=open(lock_path,O_CREAT|O_RDWR|O_CLOEXEC,0600); g_free(lock_path);
    if(lock_fd<0) { perror("lock"); return 1; }
    if(flock(lock_fd,LOCK_EX|LOCK_NB)!=0) { close(lock_fd); g_free(state_dir); return 0; }
    GdkDisplay *display=gdk_display_get_default();
    GdkMonitor *chosen=gdk_display_get_primary_monitor(display);
    if(!chosen && gdk_display_get_n_monitors(display)>0) chosen=gdk_display_get_monitor(display,0);
    if(!chosen) { fprintf(stderr,"No monitor available.\n"); close(lock_fd); g_free(state_dir); return 1; }
    monitor=g_object_ref(chosen);
    GdkRectangle geometry; gdk_monitor_get_geometry(monitor,&geometry);
    monitor_width=geometry.width; monitor_height=geometry.height;
    layout_path=g_build_filename(state_dir,"layout.ini",NULL);
    layout_defaults(&layout,monitor_width,monitor_height);
    layout_load(&layout,layout_path,monitor_width,monitor_height);
    sample();
    for(int i=0;i<PANEL_COUNT;i++) {
        windows[i]=make_layer(GTK_LAYER_SHELL_LAYER_BOTTOM,PANEL_WIDTH,panel_heights[i]);
        gpointer index=GINT_TO_POINTER(i);
        gtk_widget_add_events(windows[i],GDK_BUTTON_PRESS_MASK|GDK_BUTTON_RELEASE_MASK|GDK_POINTER_MOTION_MASK);
        g_signal_connect(windows[i],"draw",G_CALLBACK(draw),index);
        g_signal_connect(windows[i],"realize",G_CALLBACK(input_shape),index);
        g_signal_connect(windows[i],"button-press-event",G_CALLBACK(press),index);
        g_signal_connect(windows[i],"motion-notify-event",G_CALLBACK(motion),index);
        g_signal_connect(windows[i],"button-release-event",G_CALLBACK(release),index);
        g_signal_connect(windows[i],"grab-broken-event",G_CALLBACK(grab_broken),index);
        place_panel(i); gtk_widget_show_all(windows[i]);
    }
    g_signal_connect(monitor,"notify::geometry",G_CALLBACK(geometry_changed),NULL);
    g_signal_connect(display,"monitor-removed",G_CALLBACK(monitor_removed),NULL);
    g_unix_signal_add(SIGTERM,quit,NULL); g_unix_signal_add(SIGINT,quit,NULL);
    write_status();
    g_timeout_add_seconds(5,update,NULL);
    gtk_main();
    char *status_path=g_build_filename(state_dir,"status.json",NULL);
    finish_drag(FALSE);
    for(int i=0;i<PANEL_COUNT;i++) gtk_widget_destroy(windows[i]);
    unlink(status_path); g_free(status_path); g_free(layout_path); g_free(state_dir);
    g_clear_object(&monitor); close(lock_fd);
    return 0;
}
