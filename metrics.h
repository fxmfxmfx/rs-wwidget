#ifndef WIDGET_METRICS_H
#define WIDGET_METRICS_H
#include <stdbool.h>
#include <stdint.h>
typedef struct { uint64_t total, idle; } CpuSample;
int month_days(int year, int month);
int month_start(int year, int month);
bool parse_cpu(const char *text, CpuSample *sample);
bool load_cpu_sample(const char *path, CpuSample *out);
double cpu_percent(CpuSample old, CpuSample now);
bool parse_memory(const char *text, double *used_gib, double *total_gib);
#endif
