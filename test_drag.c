/* Regression: a lost release must not leave a grid permanently active.
   Also checks release routed to the overlay and its different coordinates. */
#include "drag.h"
#include <assert.h>
#include <stdio.h>
int main(void) {
    DragState state={.panel=-1};
    gint64 start=100*G_USEC_PER_SEC;
    assert(drag_press(&state,1,1,start)==DRAG_BEGIN);
    assert(state.panel==1);
    assert(drag_release(&state,-1,1)==DRAG_COMMIT); /* grid receives release */
    drag_end(&state);
    assert(drag_release(&state,-1,1)==DRAG_IGNORE);
    assert(drag_press(&state,1,1,start)==DRAG_BEGIN);
    /* No release, no motion and no broken-grab notification arrive. */
    assert(!drag_expired(&state,start+DRAG_IDLE_US-1));
    assert(drag_expired(&state,start+DRAG_IDLE_US));
    assert(drag_press(&state,-1,3,start+1)==DRAG_CANCEL); /* right click grid */
    drag_end(&state);
    assert(drag_press(&state,2,1,start)==DRAG_BEGIN);
    assert(drag_press(&state,0,1,start+1)==DRAG_CANCEL); /* next click recovers */
    assert(drag_motion(&state,0,TRUE,start+1)==DRAG_IGNORE);
    assert(drag_motion(&state,-1,TRUE,start+DRAG_IDLE_US-1)==DRAG_MOVE);
    assert(!drag_expired(&state,start+DRAG_IDLE_US)); /* movement refreshes idle deadline */
    assert(drag_motion(&state,-1,FALSE,start+DRAG_IDLE_US)==DRAG_CANCEL);
    assert(drag_release(&state,0,1)==DRAG_IGNORE);
    assert(drag_release(&state,2,1)==DRAG_COMMIT);
    drag_end(&state);
    assert(!drag_expired(&state,start+2*DRAG_IDLE_US));
    assert(drag_press(&state,-1,1,start)==DRAG_IGNORE); /* grid cannot start a drag */
    assert(drag_press(&state,0,3,start)==DRAG_IGNORE);
    DragPoint local=drag_local_point((PanelPosition){160,64},248,112,TRUE);
    assert(local.x==88 && local.y==48);
    PanelPosition drop=layout_drag((PanelPosition){160,64},24,16,local.x,local.y,1920,1080,144);
    assert(drop.x==224 && drop.y==96);
    puts("lost release / grid input / timeout / next-click recovery / coordinates: PASS");
}
