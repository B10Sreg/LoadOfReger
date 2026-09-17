# Load of Reger на NVIDIA

Рис разрабатывался на amdgpu, и всё, что в нём двигается — анимации picom,
листание тегов, прозрачность бара — на свободном драйвере работает из коробки.
На проприетарном драйвере NVIDIA три вещи ломаются предсказуемо, и все три
чинятся настройками, а не кодом:

| Симптом | Причина | Где чинить |
|---|---|---|
| Мерцание, следы окон, куски прошлого кадра | частичная перерисовка (`use-damage`) на проприетарном драйвере | [шаг 2](#2-picom-выключить-use-damage) |
| Разрыв картинки при переезде окон | нет полной композиции на стороне драйвера | [шаг 3](#3-полная-композиция-против-разрывов) |
| Чёрный экран после гибернации | видеопамять не попала в образ | [шаг 4](#4-гибернация) |

Ниже — по порядку. Всё, кроме первого шага, можно делать после установки риса.

---

## 1. Драйвер

```bash
# Turing (GTX 16xx / RTX 20xx) и новее — открытые модули, их же рекомендует NVIDIA
sudo pacman -S nvidia-open-dkms nvidia-utils lib32-nvidia-utils nvidia-settings

# Maxwell / Pascal (GTX 9xx, 10xx) — закрытые модули
sudo pacman -S nvidia-dkms nvidia-utils lib32-nvidia-utils nvidia-settings
```

`*-dkms`, а не `nvidia`: рис ставит vxwm из исходников и переживает обновления
ядра, а dkms-версия драйвера пересобирается вместе с ним и не оставляет систему
без видео после `pacman -Syu`.

Дальше — KMS. Без него X стартует на modesetting-заглушке, картинка мигает при
переключении в консоль, а SDDM иногда стартует на чёрном экране:

```bash
# /etc/mkinitcpio.conf
MODULES=(nvidia nvidia_modeset nvidia_uvm nvidia_drm)
```

и в параметры ядра — `nvidia_drm.modeset=1`. Для GRUB:

```bash
sudoedit /etc/default/grub          # GRUB_CMDLINE_LINUX_DEFAULT="... nvidia_drm.modeset=1"
sudo grub-mkconfig -o /boot/grub/grub.cfg
sudo mkinitcpio -P
```

> `vxwm/hibernate-setup.sh` делает ровно это же сам, если видит загруженный
> модуль nvidia — см. [шаг 4](#4-гибернация). Отдельно вручную настраивать
> ничего не нужно, если планируешь гибернацию.

Проверка после перезагрузки:

```bash
cat /sys/module/nvidia_drm/parameters/modeset   # должно быть Y
```

---

## 2. picom: выключить use-damage

Рис по умолчанию просит picom перерисовывать только изменившуюся часть экрана —
на amdgpu это чистый выигрыш. На проприетарном драйвере NVIDIA это же даёт
мерцание, застрявшие полосы и «призраки» окон после закрытия.

Правится одной строкой в `~/.config/vxwm-rice/rice.toml`:

```toml
[compositor]
use_damage = false
```

и применяется без перезапуска сессии:

```bash
vxwm-rice-apply        # он же vxwm/rice/apply.py
```

Если артефакты остались — попробуй второй бэкенд. `xrender` рисует на CPU:
анимации станут проще, зато исчезнут все стеклянные эффекты драйвера.

```toml
[compositor]
backend = "xrender"   # вместо "glx"
blur = false          # xrender умеет размытие, но дорого — на слабом CPU выключи
```

---

## 3. Полная композиция против разрывов

`vsync = true` в `rice.toml` — это синхронизация внутри picom. На NVIDIA её
недостаточно: разрыв возникает уже в драйвере, до композитора. Лечится
`ForceFullCompositionPipeline` на стороне X:

```bash
sudo tee /etc/X11/xorg.conf.d/20-nvidia.conf >/dev/null <<'CONF'
Section "Device"
    Identifier "NVIDIA Card"
    Driver     "nvidia"
    Option     "TripleBuffer" "on"
    Option     "AllowIndirectGLXProtocol" "off"
    # Полная композиция убирает разрывы ценой ~1 кадра задержки.
    # Для нескольких мониторов перечисли все режимы через запятую —
    # готовую строку выдаёт nvidia-settings на вкладке X Server Display.
    Option     "metamodes" "nvidia-auto-select +0+0 {ForceCompositionPipeline=On, ForceFullCompositionPipeline=On}"
EndSection
CONF
```

Перезагрузка (или перезапуск X) — и разрывы уходят.

Если после этого анимации листания тегов кажутся «резиновыми», добавь в
`~/.xprofile`:

```bash
# Не даём драйверу копить кадры: на анимациях это лишняя задержка отклика.
export __GL_MaxFramesAllowed=1
# Ожидание кадра через usleep вместо busy-wait: меньше нагрева на ноутбуке.
export __GL_YIELD=USLEEP
```

---

## 4. Гибернация

Гибернация — основной путь выхода из сессии в этом рисе (`Mod+Shift+F4` →
«Гибернация»), поэтому на NVIDIA её нужно настроить, иначе после пробуждения
вместо рабочего стола будет чёрный экран: драйвер по умолчанию не сохраняет
содержимое видеопамяти.

Скрипт риса делает всё сам и сам определяет карту:

```bash
sudo bash vxwm/hibernate-setup.sh
```

Что он при этом сделает именно на NVIDIA:

- заведёт swapfile размером с оперативную память и пропишет `resume=`/`resume_offset=`;
- **оставит** `nvidia_drm.modeset=1` в параметрах ядра (на не-NVIDIA он, наоборот, вычищает такие хвосты) — и добавит, если его там не было;
- пропишет `MODULES=(nvidia nvidia_modeset nvidia_uvm nvidia_drm)` в mkinitcpio;
- создаст `/etc/modprobe.d/nvidia-power-management.conf` с `NVreg_PreserveVideoMemoryAllocations=1`;
- включит `nvidia-suspend.service`, `nvidia-hibernate.service`, `nvidia-resume.service`.

Если карта определилась неверно (гибридный ноутбук, драйвер ещё не загружен):

```bash
sudo VXWM_GPU=nvidia bash vxwm/hibernate-setup.sh
```

Проверка после перезагрузки:

```bash
cat /proc/cmdline | tr ' ' '\n' | grep -E 'resume|nvidia'
systemctl is-enabled nvidia-hibernate.service
systemctl hibernate
```

Не проснулось — смотри лог предыдущего сеанса: `journalctl -b -1 | grep -i -E 'hibernat|nvidia'`.

---

## 5. Если что-то всё-таки не так

**Чёрный экран вместо SDDM.** Тема экрана входа рисуется Qt поверх драйвера,
который в этот момент только поднялся. Проверь `nvidia_drm.modeset=1` (шаг 1) и
убедись, что `sddm.service` включён: `systemctl status sddm`.

**picom падает на старте.** Смотри, на чём именно:

```bash
picom --config ~/.config/picom/picom.conf --log-level=debug
```

`GLX_EXT_buffer_age`, `glXCreatePixmap` в ошибках — почти всегда `use-damage`
(шаг 2) или несовпадение версий `nvidia-utils` и загруженного модуля после
обновления без перезагрузки: `nvidia-smi` скажет, если они разошлись.

**Композитор пропал посреди работы** — его сторож (`picom/picom-keeper.sh`)
поднимет его сам и покажет уведомление. Пять падений за минуту — сторож
сдаётся и пишет об этом; тогда см. `coredumpctl list | grep picom`.

**Анимации дёргаются только на одном мониторе.** Разные частоты обновления:
picom синхронизируется с одним, а рисует на оба. `nvidia-settings` → X Server
Display → выставь одинаковый режим, либо оставь композицию только на основном
через `metamodes` (шаг 3).

**Всё равно тяжело.** Рис не обязан быть с анимациями:

```toml
[compositor]
animations = false
blur = false
shadow = false
```

Бар, теги, темы и сессия от этого не зависят — они рисуются самим vxwm и vxbar.
