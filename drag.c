#include "drag.h"
static gboolean accepts(const DragState *state,int source) {
    return state->panel>=0 && (source==state->panel || source==-1);
}
DragAction drag_press(DragState *state,int panel,unsigned button,gint64 now) {
    /* A fresh click recovers a previous drag whose release never arrived. */
    if(state->panel>=0) return DRAG_CANCEL;
    if(button!=1 || panel<0 || panel>=PANEL_COUNT) return DRAG_IGNORE;
    state->panel=panel; state->last_activity=now;
    return DRAG_BEGIN;
}
DragAction drag_motion(DragState *state,int source,gboolean left_down,gint64 now) {
    if(!accepts(state,source)) return DRAG_IGNORE;
    if(!left_down) return DRAG_CANCEL;
    state->last_activity=now;
    return DRAG_MOVE;
}
DragAction drag_release(const DragState *state,int source,unsigned button) {
    return accepts(state,source) && button==1 ? DRAG_COMMIT : DRAG_IGNORE;
}
gboolean drag_expired(const DragState *state,gint64 now) {
    return state->panel>=0 && now-state->last_activity>=DRAG_IDLE_US;
}
void drag_end(DragState *state) { state->panel=-1; }
DragPoint drag_local_point(PanelPosition initial,double x,double y,gboolean overlay) {
    return (DragPoint){x-(overlay?initial.x:0),y-(overlay?initial.y:0)};
}
