FROM archlinux:latest

RUN pacman -Syu --noconfirm --needed base-devel sudo git \
    && pacman -S --noconfirm --needed \
       bzip2 gcc-libs glibc ripgrep sqlite xz zstd \
       cargo pandoc poppler \
    && useradd -m builder \
    && echo 'builder ALL=(ALL) NOPASSWD: ALL' > /etc/sudoers.d/builder \
    && rm -rf /var/cache/pacman/pkg/*

USER builder
WORKDIR /home/builder
