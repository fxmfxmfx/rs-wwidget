/* Catch needless repainting and stale minute/date/metric displays. */
#include "refresh.h"
#include <assert.h>
#include <stdio.h>
int main(void) {
    DisplayState a={.time={.tm_year=126,.tm_mon=9,.tm_mday=19,.tm_wday=1,.tm_hour=1,.tm_min=2},
        .cpu_ready=true,.memory_ready=true,.disk_ready=true,.cpu=12.34,
        .ram_used=5,.ram_total=16,.disk_free=132,.disk_used_fraction=.2};
    DisplayState b=a;
    assert(display_changes(&a,&b)==0);
    b.time.tm_sec=30;
    assert(display_changes(&a,&b)==0);
    b.time.tm_min++;
    assert(display_changes(&a,&b)==REFRESH_CLOCK);
    b=a;b.time.tm_mday++;
    assert(display_changes(&a,&b)==(REFRESH_CLOCK|REFRESH_CALENDAR));
    b=a;b.time.tm_year++;
    assert(display_changes(&a,&b)==(REFRESH_CLOCK|REFRESH_CALENDAR));
    b=a;b.cpu+=.001; /* bar can change even when displayed CPU rounds identically */
    assert(display_changes(&a,&b)==REFRESH_SYSTEM);
    b=a;b.ram_used+=.001;
    assert(display_changes(&a,&b)==REFRESH_SYSTEM);
    b=a;b.disk_used_fraction+=.001;
    assert(display_changes(&a,&b)==REFRESH_SYSTEM);
    b=a;b.disk_free+=1;
    assert(display_changes(&a,&b)==REFRESH_SYSTEM);
    b=a;b.cpu_ready=false;
    assert(display_changes(&a,&b)==REFRESH_SYSTEM);
    a=b;b.cpu=99; /* unavailable value is not visible */
    assert(display_changes(&a,&b)==0);
    puts("redraw changes: unchanged / minute / date / subpixel metrics / unavailable: PASS");
}
