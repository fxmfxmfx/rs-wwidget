#include "layout.h"
#include <math.h>
const int panel_heights[PANEL_COUNT]={144,274,178};
static const char *names[PANEL_COUNT]={"clock","calendar","system"};
int snap_axis(double value,int extent,int size,int inset) {
    int maximum=MAX(0,extent-size)/GRID_SIZE*GRID_SIZE;
    int minimum=MIN(maximum,MAX(0,inset+GRID_SIZE-1)/GRID_SIZE*GRID_SIZE);
    if (!isfinite(value)) return minimum;
    value=CLAMP(value,(double)minimum,(double)maximum);
    return CLAMP((int)(floor((value+GRID_SIZE/2)/GRID_SIZE)*GRID_SIZE),minimum,maximum);
}
PanelPosition layout_drag(PanelPosition p,double sx,double sy,double x,double y,int w,int h,int ph) {
    return (PanelPosition){snap_axis(p.x+x-sx,w,PANEL_WIDTH,0),snap_axis(p.y+y-sy,h,ph,PANEL_TOP)};
}
void layout_constrain(PanelLayout *l,int w,int h) {
    for(int i=0;i<PANEL_COUNT;i++) {
        l->pos[i].x=snap_axis(l->pos[i].x,w,PANEL_WIDTH,0);
        l->pos[i].y=snap_axis(l->pos[i].y,h,panel_heights[i],PANEL_TOP);
    }
}
void layout_defaults(PanelLayout *l,int w,int h) {
    static const int y[]={64,224,514};
    for(int i=0;i<PANEL_COUNT;i++) l->pos[i]=(PanelPosition){w-PANEL_WIDTH-40,y[i]};
    layout_constrain(l,w,h);
}
gboolean layout_load(PanelLayout *l,const char *path,int w,int h) {
    GKeyFile *file=g_key_file_new();
    if(!g_key_file_load_from_file(file,path,G_KEY_FILE_NONE,NULL)) { g_key_file_unref(file); return FALSE; }
    for(int i=0;i<PANEL_COUNT;i++) {
        GError *xe=NULL,*ye=NULL;
        int x=g_key_file_get_integer(file,names[i],"x",&xe);
        int y=g_key_file_get_integer(file,names[i],"y",&ye);
        if(!xe && !ye) l->pos[i]=(PanelPosition){x,y};
        g_clear_error(&xe); g_clear_error(&ye);
    }
    layout_constrain(l,w,h); g_key_file_unref(file); return TRUE;
}
gboolean layout_save(const PanelLayout *l,const char *path,GError **error) {
    GKeyFile *file=g_key_file_new();
    for(int i=0;i<PANEL_COUNT;i++) {
        g_key_file_set_integer(file,names[i],"x",l->pos[i].x);
        g_key_file_set_integer(file,names[i],"y",l->pos[i].y);
    }
    gboolean ok=g_key_file_save_to_file(file,path,error);
    g_key_file_unref(file); return ok;
}
