/* The helper program of the built-in browser (reelhouse-cef-helper): Chromium runs its pages, its GPU work and its
 * network in separate processes, started from this small program (CEF's settings.browser_subprocess_path) instead
 * of Reelhouse itself, so the app's own start-up never runs in them. It loads libcef from its own folder (no import
 * library: MinGW builds it) and hands the process to Chromium. */
#ifndef _WIN32
#define _GNU_SOURCE  /* readlink, PATH_MAX */
#endif
#include <stdio.h>
#include <string.h>

#include "include/cef_api_hash.h"
#include "include/capi/cef_app_capi.h"

#ifdef _WIN32
#include <windows.h>
#else
#include <dlfcn.h>
#include <limits.h>
#include <unistd.h>
#endif

/* On Linux the page processes are forked from a "zygote" deep inside cef_execute_process, and each child gets a new
 * stack canary; the frames of this file are still on the stack when it returns, so they must not check the canary
 * (Chromium's own main() is built the same way). The build adds -fno-stack-protector as well. */
#if defined(__GNUC__) && !defined(_WIN32) && (__GNUC__ >= 11 || defined(__clang__))
#define NO_STACK_PROTECTOR __attribute__((no_stack_protector))
#else
#define NO_STACK_PROTECTOR
#endif

typedef const char *(*api_hash_fn)(int, int);
typedef int (*execute_process_fn)(const cef_main_args_t *, cef_app_t *, void *);

#ifdef _WIN32
static void *load_cef(void) {
    wchar_t path[MAX_PATH + 32];
    DWORD n = GetModuleFileNameW(NULL, path, MAX_PATH);
    if (n == 0 || n >= MAX_PATH) return NULL;
    while (n > 0 && path[n - 1] != L'\\' && path[n - 1] != L'/') --n;
    wcscpy(path + n, L"libcef.dll");
    return (void *)LoadLibraryW(path);
}
static void *find(void *lib, const char *name) { return (void *)GetProcAddress((HMODULE)lib, name); }
#else
static void *load_cef(void) {
    char path[PATH_MAX + 16];
    ssize_t n = readlink("/proc/self/exe", path, PATH_MAX);
    if (n <= 0) return NULL;
    path[n] = 0;
    char *slash = strrchr(path, '/');
    if (!slash) return NULL;
    strcpy(slash + 1, "libcef.so");
    return dlopen(path, RTLD_NOW | RTLD_LOCAL);
}
static void *find(void *lib, const char *name) { return dlsym(lib, name); }
#endif

NO_STACK_PROTECTOR static int run(const cef_main_args_t *args) {
    void *lib = load_cef();
    if (!lib) {
        fprintf(stderr, "reelhouse-cef-helper: libcef was not found next to the helper\n");
        return 1;
    }
    api_hash_fn api_hash = (api_hash_fn)find(lib, "cef_api_hash");
    execute_process_fn execute_process = (execute_process_fn)find(lib, "cef_execute_process");
    if (!api_hash || !execute_process) return 1;
    /* the same API version as the app (it is checked there): this call tells libcef which one we speak */
    const char *hash = api_hash(CEF_API_VERSION, 0);
    if (!hash || strcmp(hash, CEF_API_HASH_PLATFORM) != 0) {
        fprintf(stderr, "reelhouse-cef-helper: libcef is not the version this helper was built for\n");
        return 1;
    }
    /* no app object: the pages need nothing of ours in their processes */
    int code = execute_process(args, NULL, NULL);
    return code < 0 ? 0 : code;
}

#ifdef _WIN32
int WINAPI WinMain(HINSTANCE instance, HINSTANCE prev, LPSTR cmd, int show) {
    (void)prev;
    (void)cmd;
    (void)show;
    cef_main_args_t args;
    memset(&args, 0, sizeof(args));
    args.instance = instance;
    return run(&args);
}
#else
NO_STACK_PROTECTOR int main(int argc, char **argv) {
    cef_main_args_t args;
    args.argc = argc;
    args.argv = argv;
    return run(&args);
}
#endif
