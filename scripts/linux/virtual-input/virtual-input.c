/*
 * A virtual keyboard and pointer for the headless sway session of the
 * Linux tests. The seat then has a keyboard and a pointer, so windows get
 * the focus, and tests can press keys and buttons.
 *
 *   virtual-input FIFO
 *
 * Each line of FIFO is one command. Positions are output pixels of the
 * 1280x800 layout; codes are evdev codes (BTN_LEFT is 272).
 *
 *   move X Y          moves the pointer
 *   button CODE 0|1   releases or presses a button
 *   key CODE 0|1      releases or presses a key
 *   wheel DY          scrolls DY wheel steps (positive is down)
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include <xkbcommon/xkbcommon.h>

#include "virtual-keyboard-unstable-v1-client-protocol.h"
#include "wlr-virtual-pointer-unstable-v1-client-protocol.h"

static struct wl_seat *seat;
static struct zwp_virtual_keyboard_manager_v1 *keyboard_manager;
static struct zwlr_virtual_pointer_manager_v1 *pointer_manager;

static void global(void *data, struct wl_registry *registry, uint32_t name,
                   const char *interface, uint32_t version) {
    (void)data;
    (void)version;
    if (strcmp(interface, wl_seat_interface.name) == 0 && !seat) {
        seat = wl_registry_bind(registry, name, &wl_seat_interface, 1);
    } else if (strcmp(interface, zwp_virtual_keyboard_manager_v1_interface.name) == 0) {
        keyboard_manager = wl_registry_bind(registry, name,
                                            &zwp_virtual_keyboard_manager_v1_interface, 1);
    } else if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        pointer_manager = wl_registry_bind(registry, name,
                                           &zwlr_virtual_pointer_manager_v1_interface, 1);
    }
}

static void global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {global, global_remove};

static uint32_t now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (uint32_t)(ts.tv_sec * 1000 + ts.tv_nsec / 1000000);
}

/* Sends the default US keymap to the keyboard. */
static int send_keymap(struct zwp_virtual_keyboard_v1 *keyboard) {
    struct xkb_context *context = xkb_context_new(XKB_CONTEXT_NO_FLAGS);
    if (!context) {
        return -1;
    }
    struct xkb_rule_names names = {0};
    struct xkb_keymap *keymap = xkb_keymap_new_from_names(context, &names, 0);
    if (!keymap) {
        xkb_context_unref(context);
        return -1;
    }
    char *text = xkb_keymap_get_as_string(keymap, XKB_KEYMAP_FORMAT_TEXT_V1);
    xkb_keymap_unref(keymap);
    xkb_context_unref(context);
    if (!text) {
        return -1;
    }
    size_t size = strlen(text) + 1;
    int fd = memfd_create("keymap", MFD_CLOEXEC);
    if (fd < 0 || write(fd, text, size) != (ssize_t)size) {
        free(text);
        return -1;
    }
    free(text);
    zwp_virtual_keyboard_v1_keymap(keyboard, XKB_KEYMAP_FORMAT_TEXT_V1, fd, (uint32_t)size);
    close(fd);
    return 0;
}

static void run(const char *line, struct zwp_virtual_keyboard_v1 *keyboard,
                struct zwlr_virtual_pointer_v1 *pointer) {
    unsigned a = 0, b = 0;
    double value = 0;
    if (sscanf(line, "move %u %u", &a, &b) == 2) {
        zwlr_virtual_pointer_v1_motion_absolute(pointer, now_ms(), a, b, 1280, 800);
        zwlr_virtual_pointer_v1_frame(pointer);
    } else if (sscanf(line, "button %u %u", &a, &b) == 2) {
        zwlr_virtual_pointer_v1_button(pointer, now_ms(), a, b);
        zwlr_virtual_pointer_v1_frame(pointer);
    } else if (sscanf(line, "key %u %u", &a, &b) == 2) {
        zwp_virtual_keyboard_v1_key(keyboard, now_ms(), a, b);
    } else if (sscanf(line, "wheel %lf", &value) == 1) {
        zwlr_virtual_pointer_v1_axis_source(pointer, 0);
        zwlr_virtual_pointer_v1_axis_discrete(pointer, now_ms(), 0,
                                              wl_fixed_from_double(value * 15.0), (int)value);
        zwlr_virtual_pointer_v1_frame(pointer);
    } else {
        fprintf(stderr, "virtual-input: unknown command: %s\n", line);
    }
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s FIFO\n", argv[0]);
        return 2;
    }
    /* Read and write, so the FIFO never reaches its end between writers. */
    int fifo = open(argv[1], O_RDWR | O_CLOEXEC);
    if (fifo < 0) {
        perror("virtual-input: open");
        return 1;
    }
    struct wl_display *display = wl_display_connect(NULL);
    if (!display) {
        fprintf(stderr, "virtual-input: no Wayland display\n");
        return 1;
    }
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, NULL);
    wl_display_roundtrip(display);
    if (!seat || !keyboard_manager || !pointer_manager) {
        fprintf(stderr, "virtual-input: the compositor lacks virtual input\n");
        return 1;
    }
    struct zwp_virtual_keyboard_v1 *keyboard =
        zwp_virtual_keyboard_manager_v1_create_virtual_keyboard(keyboard_manager, seat);
    struct zwlr_virtual_pointer_v1 *pointer =
        zwlr_virtual_pointer_manager_v1_create_virtual_pointer(pointer_manager, seat);
    if (send_keymap(keyboard) != 0) {
        fprintf(stderr, "virtual-input: no keymap\n");
        return 1;
    }
    wl_display_roundtrip(display);
    char buffer[4096];
    size_t used = 0;
    for (;;) {
        struct pollfd fds[2] = {
            {wl_display_get_fd(display), POLLIN, 0},
            {fifo, POLLIN, 0},
        };
        wl_display_flush(display);
        if (poll(fds, 2, -1) < 0) {
            if (errno == EINTR) {
                continue;
            }
            return 1;
        }
        if (fds[0].revents & (POLLHUP | POLLERR)) {
            return 0;
        }
        if (fds[0].revents & POLLIN && wl_display_dispatch(display) < 0) {
            return 0;
        }
        if (fds[1].revents & POLLIN) {
            ssize_t count = read(fifo, buffer + used, sizeof buffer - used - 1);
            if (count <= 0) {
                continue;
            }
            used += (size_t)count;
            buffer[used] = '\0';
            char *start = buffer;
            char *end;
            while ((end = strchr(start, '\n'))) {
                *end = '\0';
                run(start, keyboard, pointer);
                start = end + 1;
            }
            used = strlen(start);
            memmove(buffer, start, used + 1);
            if (used == sizeof buffer - 1) {
                used = 0;
            }
        }
    }
}
