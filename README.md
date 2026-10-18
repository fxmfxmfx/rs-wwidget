# Gruvbox Desktop Widgets

Небольшие виджеты рабочего стола для Wayland и MangoWM в цветах Gruvbox Material.
Часы, календарь и монитор CPU, памяти и свободного места — в одном процессе на C.

- Каждый виджет перетаскивается отдельно за заголовок или значок ≡.
- Во время захвата появляется сетка 16 px; положение привязывается к ней и сохраняется.
- Сетка создаётся только на время перетаскивания.
- Виджеты находятся под окнами приложений и не занимают место в раскладке.
- Показатели обновляются раз в 5 секунд; клики вне заголовков проходят сквозь виджеты.

## Требования

Linux, компилятор C11, pkg-config, GTK 3, gtk-layer-shell, GLib, Python 3.9+
и desktop-file-utils для проверки ярлыков. Для запуска нужен Wayland-композитор
с поддержкой layer-shell. Интеграция автозапуска рассчитана на MangoWM 0.18+.

Arch Linux / CachyOS:

```sh
sudo pacman -S --needed base-devel pkgconf gtk3 gtk-layer-shell python desktop-file-utils
```

Debian / Ubuntu:

```sh
sudo apt install build-essential pkg-config libgtk-3-dev libgtk-layer-shell-dev python3 desktop-file-utils
```

Gentoo:

Для [GTK 3](https://packages.gentoo.org/packages/x11-libs/gtk%2B) включи USE-флаг
`wayland`. Добавь следующую строку в `/etc/portage/package.use` либо в отдельный
файл `/etc/portage/package.use/desktop-widgets`, если `package.use` — каталог:

```text
x11-libs/gtk+:3 wayland
```

Установи зависимости (от root или через `sudo`):

```sh
sudo emerge --ask --changed-use sys-devel/gcc virtual/pkgconfig dev-lang/python dev-vcs/git \
    'x11-libs/gtk+:3' gui-libs/gtk-layer-shell dev-util/desktop-file-utils
```

Нужен именно GTK 3 из слота `:3`; [gtk-layer-shell](https://packages.gentoo.org/packages/gui-libs/gtk-layer-shell)
устанавливается из основного дерева Gentoo. Отдельные `-dev` пакеты не нужны.
Проверь, что доступны компилятор `cc`, Python 3.9+ и файлы для `pkg-config`:

```sh
cc --version
python3 --version
pkg-config --modversion gtk+-3.0 gtk-layer-shell-0
```

Затем выполни общие команды из раздела «Сборка и установка» ниже от обычного
пользователя. Запускай виджеты внутри Wayland-сеанса с поддержкой layer-shell;
порядок подключения автозапуска в MangoWM одинаков для всех дистрибутивов.

Для оформления используется шрифт **MxPlus IBM VGA 8x16**. Он не включён
в репозиторий. При его отсутствии Pango выберет доступную замену; семейство шрифта
можно изменить в `widgets.c`.

## Сборка и установка

```sh
git clone https://github.com/sorywet/gruvbox-desktop-widgets.git
cd gruvbox-desktop-widgets
./build.sh
./install.sh
~/.local/bin/desktop-widgets start
```

Сборка запускает проверки календаря, метрик, сетки, сохранения расположения,
управления процессом и установки/удаления во временном домашнем каталоге.
Установка выполняется без sudo в `~/.local`; она не запускает программу
и не меняет настройки композитора автоматически. Файлы `mango.conf` и ярлык
управляются установщиком и перезаписываются при обновлении; собственные правила
добавляй в основной конфиг MangoWM.

После обновления исходников:

```sh
./build.sh
./install.sh
~/.local/bin/desktop-widgets restart
```

Бинарный файл заменяется атомарно. Работающая версия продолжает работать до перезапуска.

## Автозапуск в MangoWM

Добавь в `~/.config/mango/config.conf` строку, подставив **абсолютный путь**
к своему домашнему каталогу:

```ini
source = /home/USERNAME/.local/share/desktop-widgets/mango.conf
```

Точную строку выводит установщик. Не добавляй её повторно при обновлении.
Включённый конфиг запускает виджеты при входе и отключает для них анимацию и размытие.

## Управление

```sh
~/.local/bin/desktop-widgets start
~/.local/bin/desktop-widgets stop
~/.local/bin/desktop-widgets restart
~/.local/bin/desktop-widgets toggle
~/.local/bin/desktop-widgets status
```

В меню приложений также появится «Виджеты — показать / скрыть».
Зажми левую кнопку мыши на заголовке, перемести виджет по сетке и отпусти.
Если захват завис, повторный щелчок (в том числе правой кнопкой по сетке)
отменяет перенос и возвращает виджет на прежнее место. Через 30 секунд без
движения перенос также отменяется автоматически. Сетка обрабатывает отпускание
мыши, если событие пришло ей вместо исходного виджета.
Позиции сохраняются в `$XDG_STATE_HOME/desktop-widgets/layout.ini`
(по умолчанию `~/.local/state/desktop-widgets/layout.ini`).
При уменьшении разрешения координаты ограничиваются доступным экраном.
Перенос между разными мониторами пока не предусмотрен.

## Память

`status` показывает RSS и PSS, измеренные самим процессом через `/proc/self/smaps_rollup`.
RSS включает общие библиотеки целиком, PSS — пропорциональную долю общих страниц.
Замер обновляется через 5 секунд после запуска, затем раз в минуту.
Если процесс недоступен для проверки, команда показывает возраст сохранённого замера.
Расход зависит от темы, шрифтов и композитора; постоянный расход сетки отсутствует.

## Удаление

Выполни из терминала своей графической сессии:

```sh
~/.local/bin/desktop-widgets remove
```

Команда остановит виджеты, удалит их файлы, состояние, ярлык и добавленную строку
`source` из конфига MangoWM. Остальные настройки сохраняются. Копия исходников
в каталоге Git остаётся на месте.

## Проверка без графической сессии

```sh
./build.sh
./desktop-widgets --preview /tmp/widgets.png
./desktop-widgets --preview-grid /tmp/widgets-grid.png
```

Изображения являются предпросмотром рендерера. Автоматические проверки не заменяют
проверку перетаскивания мышью в живом Wayland-сеансе.
Начало и завершение переноса пишутся в `~/.local/state/desktop-widgets/session.log`
(либо в каталог под `$XDG_STATE_HOME`). В `status.json` поле `drag_active` показывает,
была ли активна сетка во время замера.

Техническая справка: [gtk-layer-shell](https://wmww.github.io/gtk-layer-shell/).
