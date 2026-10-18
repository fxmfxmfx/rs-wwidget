/* Catch wrong snap rounding, off-screen drops, drag jump, and lost layout. */
#include "layout.h"
#include <assert.h>
#include <glib.h>
#include <stdio.h>
int main(void) {
    assert(snap_axis(23,1920,300,0)==16);
    assert(snap_axis(24,1920,300,0)==32);
    assert(snap_axis(-100,1920,300,0)==0);
    assert(snap_axis(99999,1920,300,0)==1616);
    assert(snap_axis(1,1080,144,48)==48);
    assert(snap_axis(99999,200,274,48)==0); /* undersized output */
    PanelLayout layout;
    layout_defaults(&layout,1920,1080);
    assert(layout.pos[0].x==1584 && layout.pos[0].y==64);
    assert(layout.pos[1].y==224 && layout.pos[2].y==512);
    PanelPosition drop=layout_drag((PanelPosition){160,64},20,16,68,64,1920,1080,144);
    assert(drop.x==208 && drop.y==112);
    drop=layout_drag((PanelPosition){160,64},20,16,-500,-500,1920,1080,144);
    assert(drop.x==0 && drop.y==48);
    GError *error=NULL;
    char *dir=g_dir_make_tmp("widgets-layout-XXXXXX",&error);
    assert(dir!=NULL);
    char *file=g_build_filename(dir,"layout.ini",NULL);
    layout.pos[0]=(PanelPosition){208,112};
    assert(layout_save(&layout,file,&error));
    PanelLayout restored;
    layout_defaults(&restored,1920,1080);
    assert(layout_load(&restored,file,1920,1080));
    assert(restored.pos[0].x==208 && restored.pos[0].y==112);
    assert(layout_load(&restored,file,800,600));
    assert(restored.pos[0].x==208 && restored.pos[2].x==496 && restored.pos[2].y==416);
    g_file_set_contents(file,"[clock]\nx=oops\ny=96\n[calendar]\nx=2147483647\ny=-500\n",-1,NULL);
    layout_defaults(&restored,1920,1080);
    assert(layout_load(&restored,file,1920,1080));
    assert(restored.pos[0].x==1584 && restored.pos[0].y==64); /* reject incomplete pair */
    assert(restored.pos[1].x==1616 && restored.pos[1].y==48);
    remove(file); remove(dir); g_free(file); g_free(dir);
    puts("snap / bounds / drag deltas / persistence / corrupt layout: PASS");
}
