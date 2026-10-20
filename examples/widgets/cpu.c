#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <inttypes.h>
#include <math.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#define HISTORY_SIZE 120
#define LINE_SIZE 4096
#define PATH_SIZE 1024

typedef struct { uint64_t values[8]; } CpuCounters;

static char proc_root[PATH_SIZE] = "/proc";
static double history[HISTORY_SIZE];
static size_t history_count;
static size_t history_next;

static int make_path(char *out, size_t size, const char *name)
{
    int n = snprintf(out, size, "%s/%s", proc_root, name);
    return n >= 0 && (size_t)n < size;
}

static int read_line(const char *name, char *line, size_t size)
{
    char path[PATH_SIZE];
    FILE *f;
    int ok;
    if (!make_path(path, sizeof(path), name) || (f = fopen(path, "r")) == NULL)
        return 0;
    ok = fgets(line, (int)size, f) != NULL;
    if (fclose(f) != 0) ok = 0;
    return ok;
}

static int parse_cpu(CpuCounters *out)
{
    char line[LINE_SIZE], *p;
    size_t i;
    if (!read_line("stat", line, sizeof(line)) || strncmp(line, "cpu", 3) != 0 ||
        (line[3] != ' ' && line[3] != '\t')) return 0;
    p = line + 3;
    for (i = 0; i < 8; i++) {
        char *end;
        while (*p == ' ' || *p == '\t') p++;
        if (*p < '0' || *p > '9') return 0;
        errno = 0;
        out->values[i] = (uint64_t)strtoumax(p, &end, 10);
        if (errno == ERANGE || end == p) return 0;
        p = end;
        if (*p != '\0' && *p != '\n' && *p != ' ' && *p != '\t') return 0;
    }
    return 1;
}

static int cpu_fraction(const CpuCounters *before, const CpuCounters *after, double *out)
{
    uint64_t d[8], total = 0, idle;
    size_t i;
    if (before == NULL || after == NULL) return 0;
    for (i = 0; i < 8; i++) {
        if (after->values[i] < before->values[i]) return 0;
        d[i] = after->values[i] - before->values[i];
        if (UINT64_MAX - total < d[i]) return 0;
        total += d[i];
    }
    if (total == 0 || UINT64_MAX - d[3] < d[4]) return 0;
    idle = d[3] + d[4];
    if (idle > total) return 0;
    *out = (double)(total - idle) / (double)total;
    return 1;
}

static void push_history(double value)
{
    history[history_next] = value;
    history_next = (history_next + 1) % HISTORY_SIZE;
    if (history_count < HISTORY_SIZE) history_count++;
}

static int load_average(char *out, size_t size)
{
    char line[LINE_SIZE], *p;
    double v[3];
    size_t i;
    if (!read_line("loadavg", line, sizeof(line))) return 0;
    p = line;
    for (i = 0; i < 3; i++) {
        char *end;
        while (*p == ' ' || *p == '\t') p++;
        errno = 0;
        v[i] = strtod(p, &end);
        if (end == p || errno == ERANGE || !isfinite(v[i]) || v[i] < 0) return 0;
        if (*end != '\0' && *end != '\n' && *end != ' ' && *end != '\t') return 0;
        p = end;
    }
    return snprintf(out, size, "%.2f %.2f %.2f", v[0], v[1], v[2]) < (int)size;
}

static int uptime_text(char *out, size_t size)
{
    char line[LINE_SIZE], *end;
    double seconds;
    uint64_t minutes, days, rem;
    if (!read_line("uptime", line, sizeof(line))) return 0;
    errno = 0;
    seconds = strtod(line, &end);
    if (end == line || errno == ERANGE || !isfinite(seconds) || seconds < 0 ||
        seconds / 60.0 >= (double)UINT64_MAX) return 0;
    if (*end != '\0' && *end != '\n' && *end != ' ' && *end != '\t') return 0;
    minutes = (uint64_t)(seconds / 60.0);
    days = minutes / (24 * 60);
    rem = minutes % (24 * 60);
    return snprintf(out, size, "%" PRIu64 "d %02" PRIu64 "h %02" PRIu64 "m",
                    days, rem / 60, rem % 60) < (int)size;
}

static void json_string(const char *s)
{
    const unsigned char *p = (const unsigned char *)s;
    putchar('"');
    for (; *p; p++) {
        if (*p == '"' || *p == '\\') { putchar('\\'); putchar(*p); }
        else if (*p < 0x20) printf("\\u%04x", (unsigned int)*p);
        else putchar(*p);
    }
    putchar('"');
}

static int emit_frame(const char *cpu)
{
    char load[128], uptime[128];
    int have_load = load_average(load, sizeof(load));
    int have_uptime = uptime_text(uptime, sizeof(uptime));
    size_t i, padding = HISTORY_SIZE - history_count;
    printf("{\"version\":1,\"title\":\"CPU monitor\",\"rows\":[{\"label\":\"CPU\",\"value\":");
    json_string(cpu);
    if (history_count != 0) {
        printf(",\"history\":[");
        for (i = 0; i < HISTORY_SIZE; i++) {
            if (i) putchar(',');
            if (i < padding) putchar('0');
            else printf("%.6f", history[(history_next + i) % HISTORY_SIZE]);
        }
        putchar(']');
    }
    printf("},{\"label\":\"Load avg\",\"value\":");
    json_string(have_load ? load : "—");
    printf("},{\"label\":\"Uptime\",\"value\":");
    json_string(have_uptime ? uptime : "—");
    printf("}]}\n");
    return fflush(stdout) == 0 && !ferror(stdout);
}

static int wait_until(const struct timespec *deadline)
{
    int r;
    do { r = clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, deadline, NULL); }
    while (r == EINTR);
    return r == 0;
}

int main(int argc, char **argv)
{
    CpuCounters previous;
    int have_previous;
    struct timespec deadline;
    if (argc == 3 && strcmp(argv[1], "--proc-root") == 0) {
        if (strlen(argv[2]) >= sizeof(proc_root)) {
            fprintf(stderr, "proc root path is too long\n"); return 2;
        }
        strcpy(proc_root, argv[2]);
    } else if (argc != 1) {
        fprintf(stderr, "usage: %s [--proc-root DIR]\n", argv[0]); return 2;
    }
    signal(SIGPIPE, SIG_IGN);
    have_previous = parse_cpu(&previous);
    if (!emit_frame("Sampling…")) return 0;
    if (clock_gettime(CLOCK_MONOTONIC, &deadline) != 0) return 1;
    deadline.tv_sec++;
    for (;;) {
        CpuCounters current;
        double utilization;
        int have_current;
        char cpu[32];
        struct timespec now;
        if (!wait_until(&deadline)) return 1;
        have_current = parse_cpu(&current);
        if (cpu_fraction(have_previous ? &previous : NULL,
                         have_current ? &current : NULL, &utilization)) {
            push_history(utilization);
            (void)snprintf(cpu, sizeof(cpu), "%.0f%%", utilization * 100.0);
        } else {
            (void)snprintf(cpu, sizeof(cpu), "%s",
                           have_current && history_count == 0 ? "Sampling…" : "—");
        }
        if (have_current) { previous = current; have_previous = 1; }
        else have_previous = 0;
        if (!emit_frame(cpu)) return 0;
        deadline.tv_sec++;
        if (clock_gettime(CLOCK_MONOTONIC, &now) != 0) return 1;
        if (deadline.tv_sec < now.tv_sec ||
            (deadline.tv_sec == now.tv_sec && deadline.tv_nsec <= now.tv_nsec)) {
            deadline = now;
            deadline.tv_sec++;
        }
    }
}
