#ifndef WIDGET_DRAG_H
#define WIDGET_DRAG_H
#include <glib.h>
#include "layout.h"
#define DRAG_IDLE_US (30 * G_USEC_PER_SEC)
typedef enum { DRAG_IGNORE, DRAG_BEGIN, DRAG_MOVE, DRAG_COMMIT, DRAG_CANCEL } DragAction;
typedef struct { double x,y; } DragPoint;
typedef struct { int panel; gint64 last_activity; } DragState;
DragAction drag_press(DragState *state,int panel,unsigned button,gint64 now);
DragAction drag_motion(DragState *state,int source,gboolean left_down,gint64 now);
DragAction drag_release(const DragState *state,int source,unsigned button);
gboolean drag_expired(const DragState *state,gint64 now);
void drag_end(DragState *state);
DragPoint drag_local_point(PanelPosition initial,double x,double y,gboolean overlay);
#endif
