"""Clocks the flow sets: Razor's REALTIME reads, and Node's Date.now through gettimeofday, come
from a file; the razor binary's monotonic reads, and so its timers, run ahead of real time by an
offset the flow only moves forward."""
from contextlib import contextmanager
from pathlib import Path
import subprocess
import tempfile
import time

SOURCE = r"""
#define _GNU_SOURCE
#include <time.h>
#include <sys/time.h>
#include <sys/mman.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <dlfcn.h>
static volatile long long *ahead;
__attribute__((constructor)) static void map_ahead(void) {
    char exe[4096] = {0};
    const char *path = getenv("PROBE_AHEAD");
    int file = path && readlink("/proc/self/exe", exe, sizeof exe - 1) > 0 && !strcmp(basename(exe), "razor") ? open(path, O_RDONLY) : -1;
    void *map = file < 0 ? MAP_FAILED : mmap(NULL, sizeof *ahead, PROT_READ, MAP_SHARED, file, 0);
    if (map != MAP_FAILED) ahead = map;
    if (file >= 0) close(file);
}
static long long now(void) {
    const char *path = getenv("PROBE_NOW");
    FILE *file = path ? fopen(path, "r") : NULL;
    long long at = 0;
    if (file) { fscanf(file, "%lld", &at); fclose(file); }
    return at;
}
int clock_gettime(clockid_t id, struct timespec *out) {
    static int (*real_clock)(clockid_t, struct timespec *);
    if (!real_clock) real_clock = dlsym(RTLD_NEXT, "clock_gettime");
    long long at = id == CLOCK_REALTIME ? now() : 0;
    if (at) { out->tv_sec = at / 1000; out->tv_nsec = (at % 1000) * 1000000; return 0; }
    int failed = real_clock(id, out);
    if (failed || id != CLOCK_MONOTONIC || !ahead) return failed;
    long long ms = *ahead, ns = out->tv_nsec + ms % 1000 * 1000000;
    out->tv_sec += ms / 1000 + ns / 1000000000; out->tv_nsec = ns % 1000000000;
    return 0;
}
int gettimeofday(struct timeval *out, void *zone) {
    static int (*real_time)(struct timeval *, void *);
    if (!real_time) real_time = dlsym(RTLD_NEXT, "gettimeofday");
    long long at = now();
    if (!at) return real_time(out, zone);
    out->tv_sec = at / 1000; out->tv_usec = (at % 1000) * 1000;
    return 0;
}
"""


@contextmanager
def wall_clock(still=True):
    """Yields a scratch folder, the environment that sets the clocks and `advance(ms)`, which
    puts the wall clock at the start plus `ms`; it stands still at the start until then, or with
    `still` false keeps real time until first advanced. `advance(ms, monotonic=True)` instead
    puts the razor binary's monotonic clock `ms` ahead of real time, never back."""
    with tempfile.TemporaryDirectory(prefix="razor-clock-") as scratch:
        folder = Path(scratch)
        (folder / "clock.c").write_text(SOURCE)
        subprocess.run(["cc", "-shared", "-fPIC", str(folder / "clock.c"), "-o", str(folder / "clock.so"), "-ldl"], check=True)
        epoch, wall, ahead = int(time.time()) * 1000, folder / "now", folder / "ahead"
        ahead.write_bytes(bytes(8))

        def advance(ms, monotonic=False):
            if monotonic:  # In place: a truncated file would fault the server's mapping.
                assert ms >= int.from_bytes(ahead.read_bytes(), "little"), "the monotonic clock went back"
                with ahead.open("r+b") as file:
                    file.write(int(ms).to_bytes(8, "little"))
                return
            wall.with_suffix(".next").write_text(str(epoch + ms))
            wall.with_suffix(".next").replace(wall)

        if still:
            advance(0)
        yield folder, {"LD_PRELOAD": str(folder / "clock.so"), "PROBE_NOW": str(wall), "PROBE_AHEAD": str(ahead)}, advance
