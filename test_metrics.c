/* Catch wrong leap years, Sunday-first calendars, guest CPU double counting,
   and RAM usage that ignores reclaimable caches. Hand-checked fixtures. */
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include "metrics.h"
int main(void) {
    assert(month_days(2024, 2) == 29);
    assert(month_days(2100, 2) == 28);
    assert(month_days(2000, 2) == 29);
    assert(month_start(2026, 10) == 3); /* Thursday, Monday = 0 */
    assert(month_start(2026, 2) == 6);  /* Sunday */
    assert(month_start(2026, 6) == 0);  /* Monday */
    CpuSample a = {0}, b = {0};
    assert(parse_cpu("cpu  100 10 20 300 30 5 5 0 50 0\n", &a));
    assert(a.total == 470 && a.idle == 330);
    assert(parse_cpu("cpu  130 10 30 350 40 5 5 0 60 0\n", &b));
    assert(fabs(cpu_percent(a, b) - 40.0) < 0.001);
    assert(cpu_percent(b, b) == 0);
    assert(cpu_percent(b, a) == 0);
    assert(!parse_cpu("garbage", &a));
    double used=0, total=0;
    assert(parse_memory("MemTotal:       8388608 kB\nMemFree: 1000 kB\nMemAvailable: 3145728 kB\nCached: 2000 kB\n", &used, &total));
    assert(total == 8 && used == 5);
    assert(!parse_memory("MemTotal: 0 kB\n", &used, &total));
    assert(!parse_memory("MemTotal: 10 kB\nMemAvailable: 20 kB\n", &used, &total));
    puts("calendar / CPU deltas / memory fixtures: PASS");
}
