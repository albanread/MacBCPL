// Compiled launcher for the "MacBCPL IDE.app" bundle.
//
// Modern macOS won't double-click a bundle whose CFBundleExecutable is a
// shell script (LaunchServices error -10669), so the bundle's executable
// must be a real Mach-O. This stub finds the MacBCPL repo relative to its
// own location (…/dist/MacBCPL IDE.app/Contents/MacOS/MacBCPL-IDE → repo
// is five path components up), points COCOA_SQLITE at the sibling
// cocoa_data mirror if present, then exec's the JIT driver on the IDE
// program. Relocatable with the repo; no hard-coded paths.
//
// Build:  cc -O2 -o "MacBCPL IDE.app/Contents/MacOS/MacBCPL-IDE" launcher.c
#include <limits.h>
#include <mach-o/dyld.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static void strip(char *p) {
    char *s = strrchr(p, '/');
    if (s) *s = '\0';
}

int main(void) {
    char exe[PATH_MAX];
    uint32_t sz = sizeof(exe);
    if (_NSGetExecutablePath(exe, &sz) != 0) return 1;
    char real[PATH_MAX];
    if (realpath(exe, real)) strncpy(exe, real, sizeof(exe) - 1);

    // exe = ROOT/dist/MacBCPL IDE.app/Contents/MacOS/MacBCPL-IDE
    strip(exe); // MacBCPL-IDE
    strip(exe); // MacOS
    strip(exe); // Contents
    strip(exe); // MacBCPL IDE.app
    strip(exe); // dist   -> exe is now ROOT

    char driver[PATH_MAX], prog[PATH_MAX], db[PATH_MAX];
    snprintf(driver, sizeof driver, "%s/target/debug/newbcpl-driver", exe);
    snprintf(prog, sizeof prog, "%s/examples/bcpl-ide.bcl", exe);
    snprintf(db, sizeof db, "%s/../cocoa_data/cocoa.sqlite", exe);
    if (access(db, R_OK) == 0) setenv("COCOA_SQLITE", db, 1);

    char *args[] = {driver, "run", prog, NULL};
    execv(driver, args);
    perror("execv newbcpl-driver");
    return 1;
}
