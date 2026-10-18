#ifndef WIDGET_LAYOUT_H
#define WIDGET_LAYOUT_H
#include <glib.h>
#define PANEL_COUNT 3
#define PANEL_WIDTH 300
#define GRID_SIZE 16
#define PANEL_TOP 48
typedef struct { int x,y; } PanelPosition;
typedef struct { PanelPosition pos[PANEL_COUNT]; } PanelLayout;
extern const int panel_heights[PANEL_COUNT];
int snap_axis(double value, int extent, int size, int inset);
PanelPosition layout_drag(PanelPosition initial,double start_x,double start_y,double x,double y,int width,int height,int panel_height);
void layout_defaults(PanelLayout *layout,int width,int height);
void layout_constrain(PanelLayout *layout,int width,int height);
gboolean layout_load(PanelLayout *layout,const char *path,int width,int height);
gboolean layout_save(const PanelLayout *layout,const char *path,GError **error);
#endif
