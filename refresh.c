#include "refresh.h"
unsigned display_changes(const DisplayState *a,const DisplayState *b) {
    unsigned changed=0;
    bool date=a->time.tm_year!=b->time.tm_year || a->time.tm_mon!=b->time.tm_mon || a->time.tm_mday!=b->time.tm_mday;
    if(date || a->time.tm_wday!=b->time.tm_wday || a->time.tm_hour!=b->time.tm_hour || a->time.tm_min!=b->time.tm_min) changed|=REFRESH_CLOCK;
    if(date) changed|=REFRESH_CALENDAR;
    if(a->cpu_ready!=b->cpu_ready || a->memory_ready!=b->memory_ready || a->disk_ready!=b->disk_ready ||
       (b->cpu_ready && a->cpu!=b->cpu) ||
       (b->memory_ready && (a->ram_used!=b->ram_used || a->ram_total!=b->ram_total)) ||
       (b->disk_ready && (a->disk_free!=b->disk_free || a->disk_used_fraction!=b->disk_used_fraction))) changed|=REFRESH_SYSTEM;
    return changed;
}
