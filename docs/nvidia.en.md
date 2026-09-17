# Load of Reger on NVIDIA

This rice was developed on amdgpu, where everything that moves — picom
animations, tag sliding, bar transparency — works out of the box. On the
proprietary NVIDIA driver three things break in predictable ways, and all three
are fixed by configuration, not code:

| Symptom | Cause | Fix |
|---|---|---|
| Flicker, window ghosts, leftovers of the previous frame | partial repaint (`use-damage`) on the proprietary driver | [step 2](#2-picom-turn-use-damage-off) |
| Tearing while windows move | no full composition on the driver side | [step 3](#3-full-composition-against-tearing) |
| Black screen after hibernation | video memory never made it into the image | [step 4](#4-hibernation) |

In order. Everything but step 1 can be done after installing the rice.

---

## 1. Driver

```bash
# Turing (GTX 16xx / RTX 20xx) and newer — open modules, NVIDIA's own recommendation
sudo pacman -S nvidia-open-dkms nvidia-utils lib32-nvidia-utils nvidia-settings

# Maxwell / Pascal (GTX 9xx, 10xx) — closed modules
sudo pacman -S nvidia-dkms nvidia-utils lib32-nvidia-utils nvidia-settings
```

`*-dkms` rather than `nvidia`: the rice builds vxwm from source and survives
kernel upgrades; the dkms driver rebuilds along with the kernel instead of
leaving you without video after `pacman -Syu`.

Next, KMS. Without it X starts on the modesetting stub, the picture blinks when
you switch to a console, and SDDM sometimes comes up black:

```bash
# /etc/mkinitcpio.conf
MODULES=(nvidia nvidia_modeset nvidia_uvm nvidia_drm)
```

and add `nvidia_drm.modeset=1` to the kernel command line. For GRUB:

```bash
sudoedit /etc/default/grub          # GRUB_CMDLINE_LINUX_DEFAULT="... nvidia_drm.modeset=1"
sudo grub-mkconfig -o /boot/grub/grub.cfg
sudo mkinitcpio -P
```

> `vxwm/hibernate-setup.sh` does exactly this on its own when it sees the nvidia
> module loaded — see [step 4](#4-hibernation). If you plan to use hibernation,
> you do not need to do it by hand.

After a reboot:

```bash
cat /sys/module/nvidia_drm/parameters/modeset   # expect Y
```

---

## 2. picom: turn use-damage off

By default the rice asks picom to repaint only the part of the screen that
changed — a clear win on amdgpu. On the proprietary NVIDIA driver the same
setting produces flicker, stuck stripes and "ghosts" of closed windows.

One line in `~/.config/vxwm-rice/rice.toml`:

```toml
[compositor]
use_damage = false
```

Applied without restarting the session:

```bash
vxwm-rice-apply        # a.k.a. vxwm/rice/apply.py
```

If artifacts persist, try the other backend. `xrender` paints on the CPU:
animations get simpler, but every driver-side glass effect disappears with them.

```toml
[compositor]
backend = "xrender"   # instead of "glx"
blur = false          # xrender can blur, but it is expensive on a weak CPU
```

---

## 3. Full composition against tearing

`vsync = true` in `rice.toml` is synchronization inside picom. On NVIDIA that is
not enough: tearing happens in the driver, before the compositor ever sees the
frame. The cure is `ForceFullCompositionPipeline` on the X side:

```bash
sudo tee /etc/X11/xorg.conf.d/20-nvidia.conf >/dev/null <<'CONF'
Section "Device"
    Identifier "NVIDIA Card"
    Driver     "nvidia"
    Option     "TripleBuffer" "on"
    Option     "AllowIndirectGLXProtocol" "off"
    # Full composition removes tearing at the cost of roughly one frame of latency.
    # For several monitors list every mode, comma-separated — nvidia-settings
    # prints a ready-made string on the X Server Display tab.
    Option     "metamodes" "nvidia-auto-select +0+0 {ForceCompositionPipeline=On, ForceFullCompositionPipeline=On}"
EndSection
CONF
```

Reboot (or restart X) and the tearing is gone.

If tag-slide animations now feel rubbery, add this to `~/.xprofile`:

```bash
# Don't let the driver queue frames: on animations that is pure input latency.
export __GL_MaxFramesAllowed=1
# Wait for the frame with usleep instead of a busy loop: less heat on a laptop.
export __GL_YIELD=USLEEP
```

---

## 4. Hibernation

Hibernation is the primary way to leave a session in this rice (`Mod+Shift+F4` →
first entry), so on NVIDIA it has to be set up — otherwise waking up gives you a
black screen instead of a desktop: by default the driver does not preserve the
contents of video memory.

The rice's script does all of it and detects the card itself:

```bash
sudo bash vxwm/hibernate-setup.sh
```

What it does specifically on NVIDIA:

- creates a swapfile the size of your RAM and writes `resume=`/`resume_offset=`;
- **keeps** `nvidia_drm.modeset=1` on the kernel command line (on non-NVIDIA systems it strips such leftovers instead) — and adds it when missing;
- sets `MODULES=(nvidia nvidia_modeset nvidia_uvm nvidia_drm)` in mkinitcpio;
- writes `/etc/modprobe.d/nvidia-power-management.conf` with `NVreg_PreserveVideoMemoryAllocations=1`;
- enables `nvidia-suspend.service`, `nvidia-hibernate.service`, `nvidia-resume.service`.

If the card is detected wrong (hybrid laptop, driver not loaded yet):

```bash
sudo VXWM_GPU=nvidia bash vxwm/hibernate-setup.sh
```

After a reboot:

```bash
cat /proc/cmdline | tr ' ' '\n' | grep -E 'resume|nvidia'
systemctl is-enabled nvidia-hibernate.service
systemctl hibernate
```

Didn't wake up? Read the previous boot's log:
`journalctl -b -1 | grep -i -E 'hibernat|nvidia'`.

---

## 5. When something is still off

**Black screen instead of SDDM.** The greeter is drawn by Qt on top of a driver
that has only just come up. Check `nvidia_drm.modeset=1` (step 1) and that
`sddm.service` is enabled: `systemctl status sddm`.

**picom dies at startup.** Find out where:

```bash
picom --config ~/.config/picom/picom.conf --log-level=debug
```

`GLX_EXT_buffer_age` or `glXCreatePixmap` in the errors is almost always
`use-damage` (step 2), or a mismatch between `nvidia-utils` and the loaded
module after an upgrade without a reboot — `nvidia-smi` will say so.

**The compositor vanished mid-session.** Its watchdog
(`picom/picom-keeper.sh`) brings it back and shows a notification. Five crashes
in a minute and the watchdog gives up and says so; then check
`coredumpctl list | grep picom`.

**Animations stutter on one monitor only.** Different refresh rates: picom syncs
to one and paints on both. Either equalize the modes in `nvidia-settings` → X
Server Display, or keep composition on the primary output only via `metamodes`
(step 3).

**Still too heavy.** The rice does not need animations to work:

```toml
[compositor]
animations = false
blur = false
shadow = false
```

The bar, tags, themes and session handling do not depend on them — vxwm and
vxbar draw those themselves.
