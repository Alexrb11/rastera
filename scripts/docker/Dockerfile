# Runtime testing only: the release binary is supplied through a read-only mount.
ARG BASE_IMAGE=ubuntu:24.04
FROM ${BASE_IMAGE}
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates python3 python3-pil binutils \
    libvulkan1 mesa-vulkan-drivers vulkan-tools \
    libxkbcommon0 libxkbcommon-x11-0 libx11-6 libx11-xcb1 libxcursor1 libxi6 libxcb1 \
    libwayland-client0 libegl1 libgl1 \
    dbus dbus-x11 xdg-utils xdg-desktop-portal xdg-desktop-portal-gtk zenity \
    xvfb x11-utils xdotool openbox x11vnc novnc websockify imagemagick \
    weston libegl-mesa0 \
    fonts-dejavu-core fonts-liberation \
    && if apt-cache show wayland-utils > /dev/null 2>&1; then \
       apt-get install -y --no-install-recommends wayland-utils; fi \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 --shell /bin/bash tester \
    && mkdir -p /tmp/.X11-unix && chmod 1777 /tmp/.X11-unix
COPY entrypoint.py /opt/rastera-test/entrypoint.py
RUN install -d -o 10001 -g 10001 -m 0700 /tmp/rastera-runtime
ENV HOME=/home/tester LANG=C.UTF-8 LC_ALL=C.UTF-8 DISPLAY=:99 \
    XDG_RUNTIME_DIR=/tmp/rastera-runtime XDG_CURRENT_DESKTOP=GNOME \
    LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe PYTHONDONTWRITEBYTECODE=1
USER 10001:10001
WORKDIR /home/tester
ENTRYPOINT ["dbus-run-session", "--", "python3", "/opt/rastera-test/entrypoint.py"]
