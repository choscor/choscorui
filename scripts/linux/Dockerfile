# The Linux verification image. CI and the maintainer's Mac run the same
# image: scripts/linux/run-in-container.sh builds it and runs a job in it.
# It is multi-arch (x86_64 and aarch64) and has the Q11 floor: Ubuntu 22.04.
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8

# Build tools, the -dev packages of the binding check, the display servers
# (sway, Xvfb), Mesa llvmpipe for EGL/GLES, D-Bus with the AT-SPI registry,
# the portal, the image libraries, and fonts for the text tests.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential ca-certificates curl git pkg-config xz-utils \
        meson ninja-build \
        libwayland-dev libwayland-bin libxkbcommon-dev libxkbcommon-x11-dev \
        libxcb1-dev libxcb-xkb-dev libxcb-xinput-dev libxcb-shm0-dev \
        libxcb-randr0-dev libxcb-render0-dev libxcb-present-dev libxcb-cursor-dev \
        libxcb-xtest0-dev \
        libx11-dev libx11-xcb-dev \
        libegl-dev libgles-dev libegl1 libgles2 libegl-mesa0 libgl1-mesa-dri \
        libcairo2-dev libfreetype-dev libharfbuzz-dev libfontconfig-dev \
        libdbus-1-dev dbus dbus-x11 at-spi2-core xdg-desktop-portal \
        libpng-dev libturbojpeg0-dev libgif-dev libtiff-dev libwebp-dev \
        libheif-dev libcanberra-dev \
        sway xvfb x11-xserver-utils xauth \
        fonts-dejavu-core fonts-liberation2 \
    && rm -rf /var/lib/apt/lists/*

# Ubuntu 22.04 ships wayland-protocols 1.25. The binding check compares the
# hand-written interface tables with the protocol XML, and some protocols
# (fractional-scale, cursor-shape, xdg-dialog, xdg-system-bell) are newer.
ARG WAYLAND_PROTOCOLS=1.38
RUN curl -fsSL "https://gitlab.freedesktop.org/wayland/wayland-protocols/-/releases/${WAYLAND_PROTOCOLS}/downloads/wayland-protocols-${WAYLAND_PROTOCOLS}.tar.xz" \
        -o /tmp/wp.tar.xz \
    && mkdir /tmp/wp && tar -xJf /tmp/wp.tar.xz -C /tmp/wp --strip-components=1 \
    && meson setup /tmp/wp/build /tmp/wp --prefix=/usr -Dtests=false \
    && ninja -C /tmp/wp/build install \
    && rm -rf /tmp/wp /tmp/wp.tar.xz

# The binding check also compares the core protocol tables with wayland.xml.
# Ubuntu 22.04 ships wayland 1.20; wl_surface version 6 needs a newer file.
# Only the XML is installed, under /usr/local; the library stays 1.20.
ARG WAYLAND=1.23.0
RUN curl -fsSL "https://gitlab.freedesktop.org/wayland/wayland/-/releases/${WAYLAND}/downloads/wayland-${WAYLAND}.tar.xz" \
        -o /tmp/wayland.tar.xz \
    && mkdir -p /usr/local/share/wayland \
    && tar -xJf /tmp/wayland.tar.xz -C /tmp "wayland-${WAYLAND}/protocol/wayland.xml" \
    && mv "/tmp/wayland-${WAYLAND}/protocol/wayland.xml" /usr/local/share/wayland/wayland.xml \
    && rm -rf /tmp/wayland.tar.xz "/tmp/wayland-${WAYLAND}"

ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH
RUN curl -fsSL https://sh.rustup.rs | sh -s -- -y --no-modify-path --profile minimal \
        --default-toolchain stable --component rustfmt,clippy \
    && rustup toolchain install 1.88 --profile minimal \
    && chmod -R a+w "$RUSTUP_HOME" "$CARGO_HOME"

WORKDIR /work
