/* Render production and a frozen v3 reference on identical Cairo/Pango contexts.
 * Pixel checks compare native ARGB bytes; no golden font-version assumptions. */
#define main widget_application_main
#include "../widgets.c"
#undef main
#include "render_reference.h"
#include <assert.h>
#include <stdlib.h>

static void fixture(int mode) {
    now=(struct tm){.tm_year=126,.tm_mon=9,.tm_mday=19,.tm_wday=1,.tm_hour=1,.tm_min=23};
    cpu_ready=memory_ready=disk_ready=mode!=1;
    cpu=12.345;ram_used=5.2;ram_total=15.5;disk_free=132.4;disk_used_fraction=.321;
    if(mode==2) now=(struct tm){.tm_year=124,.tm_mon=1,.tm_mday=29,.tm_wday=4,.tm_hour=23,.tm_min=59};
}
static cairo_surface_t *image(int width,int height,double scale) {
    cairo_surface_t *surface=cairo_image_surface_create(CAIRO_FORMAT_ARGB32,width*scale,height*scale);
    cairo_surface_set_device_scale(surface,scale,scale);
    return surface;
}
static void assert_pixels(cairo_surface_t *a,cairo_surface_t *b,const char *label) {
    cairo_surface_flush(a);cairo_surface_flush(b);
    int size=cairo_image_surface_get_stride(a)*cairo_image_surface_get_height(a);
    if(memcmp(cairo_image_surface_get_data(a),cairo_image_surface_get_data(b),size)) {
        cairo_surface_write_to_png(a,"/tmp/widgets-actual.png");
        cairo_surface_write_to_png(b,"/tmp/widgets-reference.png");
        fprintf(stderr,"Pixel mismatch: %s (images in /tmp/widgets-{actual,reference}.png)\n",label);
        abort();
    }
}
static void panel_pixels(void) {
    for(int mode=0;mode<3;mode++) for(int scale=1;scale<=2;scale++) for(int panel=0;panel<3;panel++) {
        fixture(mode);
        cairo_surface_t *a=image(WIDTH,panel_heights[panel],scale),*b=image(WIDTH,panel_heights[panel],scale);
        cairo_t *ca=cairo_create(a),*cb=cairo_create(b);
        render_panel(ca,panel);reference_render_panel(cb,panel);
        assert_pixels(a,b,"panel");
        cairo_destroy(ca);cairo_destroy(cb);cairo_surface_destroy(a);cairo_surface_destroy(b);
    }
}
static void grid_frame(cairo_t *cr,int w,int h,int panel,PanelPosition p,gboolean reference) {
    clear_surface(cr);
    if(reference) reference_render_grid(cr,w,h); else render_grid(cr,w,h);
    cairo_save(cr);cairo_translate(cr,p.x,p.y);
    if(reference) reference_render_panel(cr,panel); else render_panel(cr,panel);
    color(cr,ACCENT,1);cairo_set_line_width(cr,2);
    cairo_rectangle(cr,1,1,PANEL_WIDTH-2,panel_heights[panel]-2);cairo_stroke(cr);
    cairo_restore(cr);
}
static void grid_pixels(void) {
    const PanelPosition steps[]={{320,64},{320,64},{336,80},{0,48},{640,256},{16,320}};
    fixture(0);
    for(int scale=1;scale<=2;scale++) {
        cairo_surface_t *incremental=image(960,640,scale);
        cairo_t *ci=cairo_create(incremental);
        for(unsigned step=0;step<G_N_ELEMENTS(steps);step++) {
            cairo_surface_t *expected=image(960,640,scale);
            cairo_t *ce=cairo_create(expected);
            grid_frame(ce,960,640,1,steps[step],TRUE);
            cairo_save(ci);
            if(step) {
                PanelPosition old=steps[step-1],pos=steps[step];
                PanelDamage a=layout_damage(old,panel_heights[1],960,640);
                PanelDamage b=layout_damage(pos,panel_heights[1],960,640);
                cairo_rectangle(ci,a.x,a.y,a.width,a.height);
                cairo_rectangle(ci,b.x,b.y,b.width,b.height);
                cairo_clip(ci);
            }
            grid_frame(ci,960,640,1,steps[step],FALSE);
            cairo_restore(ci);
            assert_pixels(incremental,expected,"grid damage");
            cairo_destroy(ce);cairo_surface_destroy(expected);
        }
        cairo_destroy(ci);cairo_surface_destroy(incremental);
    }
}
static double benchmark(gboolean reference,gboolean optimized_cadence) {
    fixture(0);
    cairo_surface_t *surface=image(WIDTH,HEIGHT,1);
    cairo_t *cr=cairo_create(surface);
    DisplayState before=display_state();
    struct timespec start,finish; clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&start);
    for(int minute=0;minute<20;minute++) for(int tick=0;tick<12;tick++) {
        now.tm_min=(23+minute)%60;cpu=12.345+tick;
        DisplayState after=display_state();
        unsigned changed=display_changes(&before,&after);
        if(minute==0 && tick==0) changed=7;
        before=after;
        for(int panel=0;panel<3;panel++) {
            if(optimized_cadence && !(changed&(1u<<panel))) continue;
            if(reference) reference_render_panel(cr,panel);else render_panel(cr,panel);
        }
    }
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&finish);
    double elapsed=(finish.tv_sec-start.tv_sec)*1000.+(finish.tv_nsec-start.tv_nsec)/1e6;
    cairo_destroy(cr);cairo_surface_destroy(surface);
    return elapsed;
}
static double benchmark_drag(gboolean reference) {
    fixture(0);
    const int width=1920,height=1080;
    cairo_surface_t *surface=image(width,height,1);
    cairo_t *cr=cairo_create(surface);
    PanelPosition old={320,64};
    grid_frame(cr,width,height,1,old,reference);
    struct timespec start,finish;clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&start);
    for(int frame=0;frame<120;frame++) {
        PanelPosition next={320+(frame%60)*16,64+(frame%12)*16};
        cairo_save(cr);
        if(!reference) {
            PanelDamage a=layout_damage(old,panel_heights[1],width,height);
            PanelDamage b=layout_damage(next,panel_heights[1],width,height);
            cairo_rectangle(cr,a.x,a.y,a.width,a.height);
            cairo_rectangle(cr,b.x,b.y,b.width,b.height);
            cairo_clip(cr);
        }
        grid_frame(cr,width,height,1,next,reference);
        cairo_restore(cr);old=next;
    }
    clock_gettime(CLOCK_PROCESS_CPUTIME_ID,&finish);
    double elapsed=(finish.tv_sec-start.tv_sec)*1000.+(finish.tv_nsec-start.tv_nsec)/1e6;
    cairo_destroy(cr);cairo_surface_destroy(surface);
    return elapsed;
}
int main(int argc,char **argv) {
    panel_pixels();grid_pixels();
    puts("render pixels: panels + clipped drag grid at 1x/2x match v3 exactly: PASS");
    if(argc>1 && strcmp(argv[1],"--benchmark")==0) {
        double old=benchmark(TRUE,FALSE),same=benchmark(FALSE,FALSE),updated=benchmark(FALSE,TRUE);
        printf("Headless render CPU-work benchmark (20 simulated minutes): v3 %.1f ms, optimized same draws %.1f ms, optimized redraw cadence %.1f ms\n",old,same,updated);
        double old_drag=benchmark_drag(TRUE),new_drag=benchmark_drag(FALSE);
        printf("Headless drag CPU-work benchmark (120 frames, 1920x1080): v3 %.1f ms, optimized %.1f ms\n",old_drag,new_drag);
    }
    clear_render_resources();
    return 0;
}
