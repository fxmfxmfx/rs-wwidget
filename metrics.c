#define _POSIX_C_SOURCE 200809L
#include "metrics.h"
#include <errno.h>
#include <fcntl.h>
#include <inttypes.h>
#include <stdio.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
int month_days(int y, int m) {
    static const int days[] = {31,28,31,30,31,30,31,31,30,31,30,31};
    if (m < 1 || m > 12) return 0;
    return days[m-1] + (m == 2 && y % 4 == 0 && (y % 100 != 0 || y % 400 == 0));
}
int month_start(int y, int m) {
    struct tm date = {.tm_year=y-1900, .tm_mon=m-1, .tm_mday=1, .tm_hour=12, .tm_isdst=-1};
    if (mktime(&date) == (time_t)-1) return -1;
    return (date.tm_wday + 6) % 7;
}
bool parse_cpu(const char *s, CpuSample *c) {
    uint64_t a[8] = {0};
    if (sscanf(s, "cpu %"SCNu64" %"SCNu64" %"SCNu64" %"SCNu64" %"SCNu64" %"SCNu64" %"SCNu64" %"SCNu64,
               &a[0],&a[1],&a[2],&a[3],&a[4],&a[5],&a[6],&a[7]) < 4) return false;
    c->total = 0;
    for (int i=0;i<8;i++) c->total += a[i];
    c->idle = a[3]+a[4]; /* Guest time is already included in user/nice. */
    return true;
}
bool load_cpu_sample(const char *path, CpuSample *out) {
    char line[256];
    size_t used = 0;
    bool complete = false;
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0) return false;
    while (used < sizeof line - 1) {
        ssize_t n = read(fd, line + used, sizeof line - 1 - used);
        if (n < 0) {
            if (errno == EINTR) continue;
            close(fd);
            return false;
        }
        if (n == 0) break;
        char *newline = memchr(line + used, '\n', (size_t)n);
        if (newline) {
            used = (size_t)(newline - line);
            complete = true;
            break;
        }
        used += (size_t)n;
    }
    close(fd);
    if (!complete) return false;
    line[used] = '\0';
    CpuSample sample;
    if (!parse_cpu(line, &sample)) return false;
    *out = sample;
    return true;
}
double cpu_percent(CpuSample old, CpuSample now) {
    if (now.total <= old.total || now.idle < old.idle) return 0;
    double total = now.total-old.total, idle = now.idle-old.idle;
    if (idle > total) return 0;
    return 100.0*(total-idle)/total;
}
bool parse_memory(const char *s, double *u, double *t) {
    uint64_t total=0, avail=0;
    bool found=false;
    while (*s) {
        if (sscanf(s, "MemTotal: %"SCNu64, &total) == 1) {}
        else if (sscanf(s, "MemAvailable: %"SCNu64, &avail) == 1) found=true;
        const char *next = strchr(s, '\n');
        if (!next) break;
        s=next+1;
    }
    if (!total || !found || avail > total) return false;
    *t=total/1048576.0; *u=(total-avail)/1048576.0;
    return true;
}
