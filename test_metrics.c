#define _POSIX_C_SOURCE 200809L
/* Catch wrong leap years, Sunday-first calendars, guest CPU double counting,
   and RAM usage that ignores reclaimable caches. Hand-checked fixtures. */
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include "metrics.h"

static void write_fixture(char *path, const char *line, size_t body_size) {
    int fd = mkstemp(path);
    assert(fd >= 0);
    size_t len = strlen(line), off = 0;
    while (off < len) {
        ssize_t n = write(fd, line + off, len - off);
        assert(n > 0);
        off += (size_t)n;
    }
    char body[4096];
    memset(body, 'x', sizeof body);
    while (body_size) {
        size_t amount = body_size < sizeof body ? body_size : sizeof body;
        off = 0;
        while (off < amount) {
            ssize_t n = write(fd, body + off, amount - off);
            assert(n > 0);
            off += (size_t)n;
        }
        body_size -= amount;
    }
    assert(close(fd) == 0);
}

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

    char path[] = "/tmp/widget-cpu-XXXXXX";
    write_fixture(path, "cpu 100 10 20 300 30 5 5 0 50 0\n", 1024 * 1024);
    CpuSample loaded = {0};
    assert(load_cpu_sample(path, &loaded));
    assert(loaded.total == 470 && loaded.idle == 330);
    assert(unlink(path) == 0);
    assert(!load_cpu_sample(path, &loaded));

    char invalid_path[] = "/tmp/widget-cpu-XXXXXX";
    write_fixture(invalid_path, "cpu 1 2 3\n", 0);
    assert(!load_cpu_sample(invalid_path, &loaded));
    assert(unlink(invalid_path) == 0);

    char overlong_path[] = "/tmp/widget-cpu-XXXXXX";
    char overlong_line[2048];
    memset(overlong_line, '7', sizeof overlong_line - 1);
    overlong_line[sizeof overlong_line - 2] = '\n';
    overlong_line[sizeof overlong_line - 1] = '\0';
    write_fixture(overlong_path, overlong_line, 0);
    assert(!load_cpu_sample(overlong_path, &loaded));
    assert(unlink(overlong_path) == 0);

    char truncated_path[] = "/tmp/widget-cpu-XXXXXX";
    write_fixture(truncated_path, "cpu 100 10 20 300", 0);
    assert(!load_cpu_sample(truncated_path, &loaded));
    assert(unlink(truncated_path) == 0);
    double used=0, total=0;
    assert(parse_memory("MemTotal:       8388608 kB\nMemFree: 1000 kB\nMemAvailable: 3145728 kB\nCached: 2000 kB\n", &used, &total));
    assert(total == 8 && used == 5);
    assert(!parse_memory("MemTotal: 0 kB\n", &used, &total));
    assert(!parse_memory("MemTotal: 10 kB\nMemAvailable: 20 kB\n", &used, &total));
    puts("calendar / CPU deltas / memory fixtures: PASS");
}
