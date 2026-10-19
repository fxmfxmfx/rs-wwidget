/* Frozen legacy rendering oracle, built only by the Rust integration test. */
#include <cairo.h>
#include <pango/pangocairo.h>
#include <stdio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define WIDTH 300
#define PANEL_WIDTH 300
#define GRID_SIZE 16
static const int panel_heights[] = {144, 274, 178};
static struct tm now;
static int cpu_ready, memory_ready, disk_ready;
static double cpu, ram_used, ram_total, disk_free, disk_used_fraction;
static int month_days(int year, int month) {
    static const int ds[] = {31,28,31,30,31,30,31,31,30,31,30,31};
    if (month < 1 || month > 12) return 0;
    return ds[month-1] + (month == 2 && year % 4 == 0 && (year % 100 != 0 || year % 400 == 0));
}
static int month_start(int year, int month) {
    struct tm t={.tm_year=year-1900,.tm_mon=month-1,.tm_mday=1,.tm_hour=12,.tm_isdst=-1};
    if (mktime(&t)==(time_t)-1) return -1;
    return (t.tm_wday+6)%7;
}
#include "../render_reference.h"

static void fixture(int mode) {
    now=(struct tm){.tm_year=126,.tm_mon=9,.tm_mday=19,.tm_wday=1,.tm_hour=1,.tm_min=23};
    cpu_ready=memory_ready=disk_ready=mode!=1;
    cpu=12.345;ram_used=5.2;ram_total=15.5;disk_free=132.4;disk_used_fraction=.321;
    if(mode==2) now=(struct tm){.tm_year=124,.tm_mon=1,.tm_mday=29,.tm_wday=4,.tm_hour=23,.tm_min=59};
}
static void clear_surface(cairo_t *cr) {
    cairo_set_operator(cr,CAIRO_OPERATOR_SOURCE); cairo_set_source_rgba(cr,0,0,0,0); cairo_paint(cr); cairo_set_operator(cr,CAIRO_OPERATOR_OVER);
}
static void grid_frame(cairo_t *cr,int w,int h,int panel,int x,int y) {
    clear_surface(cr); reference_render_grid(cr,w,h);
    cairo_save(cr);cairo_translate(cr,x,y);reference_render_panel(cr,panel);
    reference_color(cr,reference_ACCENT,1);cairo_set_line_width(cr,2);cairo_rectangle(cr,1,1,PANEL_WIDTH-2,panel_heights[panel]-2);cairo_stroke(cr);cairo_restore(cr);
}
int main(int argc,char **argv) {
    if(argc!=5) return 2;
    const char *kind=argv[1]; int mode=atoi(argv[2]), scale=atoi(argv[3]), index=atoi(argv[4]);
    fixture(mode);
    int w=!strcmp(kind,"panel")?WIDTH:960, h=!strcmp(kind,"panel")?panel_heights[index]:640;
    cairo_surface_t *s=cairo_image_surface_create(CAIRO_FORMAT_ARGB32,w*scale,h*scale);
    cairo_surface_set_device_scale(s,scale,scale); cairo_t *cr=cairo_create(s);
    if(!strcmp(kind,"panel")) reference_render_panel(cr,index);
    else if(!strcmp(kind,"grid")) grid_frame(cr,w,h,index,320,64);
    else return 2;
    cairo_surface_write_to_png(s,"/dev/stdout");
    cairo_destroy(cr);cairo_surface_destroy(s);return 0;
}
