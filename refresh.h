#ifndef WIDGET_REFRESH_H
#define WIDGET_REFRESH_H
#include <stdbool.h>
#include <time.h>
enum { REFRESH_CLOCK=1, REFRESH_CALENDAR=2, REFRESH_SYSTEM=4 };
typedef struct {
    struct tm time;
    bool cpu_ready,memory_ready,disk_ready;
    double cpu,ram_used,ram_total,disk_free,disk_used_fraction;
} DisplayState;
unsigned display_changes(const DisplayState *before,const DisplayState *after);
#endif
