#!/usr/bin/python3
"""Control the small Gruvbox desktop widgets; no background controller."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

home = Path.home()
share = home / '.local/share/desktop-widgets'
state = Path(os.environ.get('XDG_STATE_HOME', home / '.local/state')) / 'desktop-widgets'
binary = share / 'desktop-widgets'


def running_pid():
    try:
        info = json.loads((state / 'status.json').read_text())
        pid = int(info['pid'])
        if pid <= 1:
            return None
        executable = os.readlink(f'/proc/{pid}/exe')
        if executable.removesuffix(' (deleted)') != str(binary.resolve()):
            return None
        return pid
    except (OSError, ValueError, KeyError, TypeError):
        return None


def start():
    if running_pid():
        print('Виджеты уже работают.')
        return
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (state / 'session.log').open('w') as log:
        child = subprocess.Popen([str(binary)], stdout=log, stderr=log,
                                 start_new_session=True, close_fds=True)
    for _ in range(20):
        if running_pid():
            print('Виджеты запущены. Повторное открытие ярлыка скроет их.')
            return
        if child.poll() is not None:
            if child.returncode != 0:
                raise RuntimeError((state / 'session.log').read_text().strip())
            print('Виджеты уже запущены.')
            return
        time.sleep(0.1)
    raise RuntimeError('Не удалось подтвердить запуск. См. ' + str(state / 'session.log'))


def stop():
    pid = running_pid()
    if pid:
        os.kill(pid, signal.SIGTERM)
        for _ in range(30):
            if not running_pid():
                print('Виджеты остановлены.')
                return
            time.sleep(0.1)
        raise RuntimeError('Не удалось подтвердить остановку; файлы оставлены.')
    print('Виджеты не запущены в доступной сессии.')


def restart():
    stop()
    start()


def remove():
    stop()
    # A process outside the current PID namespace may still hold the lock.
    # Refuse removal rather than leave a live, unmanageable process behind.
    import fcntl
    lock = None
    if (state / 'lock').exists():
        lock = (state / 'lock').open('a')
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            lock.close()
            raise RuntimeError('Виджеты ещё работают. Выполни эту команду в терминале рабочего стола.')
    path = home / '.config/mango/config.conf'
    source_line = 'source = ' + str(share / 'mango.conf')
    if path.exists():
        text = path.read_text()
        lines = [line for line in text.splitlines(keepends=True)
                 if line.strip() != source_line]
        if ''.join(lines) != text:
            path.write_text(''.join(lines))
    for path in (home / 'Desktop/desktop-widgets.desktop',
                 home / '.local/share/applications/desktop-widgets.desktop'):
        path.unlink(missing_ok=True)
    shutil.rmtree(share)
    shutil.rmtree(state, ignore_errors=True)
    Path(__file__).unlink()
    if lock is not None:
        lock.close()
    print('Виджеты и их автозапуск удалены.')


def main():
    action = sys.argv[1] if len(sys.argv) > 1 else 'toggle'
    if action == 'start':
        start()
    elif action == 'stop':
        stop()
    elif action == 'restart':
        restart()
    elif action == 'toggle':
        stop() if running_pid() else start()
    elif action == 'status':
        if running_pid():
            info = json.loads((state / 'status.json').read_text())
            print(f"RSS: {info['rss_kib']/1024:.1f} MiB; PSS: {info['pss_kib']/1024:.1f} MiB. Обновляется раз в минуту.")
        else:
            try:
                info = json.loads((state / 'status.json').read_text())
                updated = int(info['updated'])
                age = max(0, int(time.time()) - updated)
                stamp = time.strftime('%Y-%m-%d %H:%M:%S %Z', time.localtime(updated))
                print(f"Процесс недоступен для проверки. Сохранённый замер: RSS {info['rss_kib']/1024:.1f} MiB; PSS {info['pss_kib']/1024:.1f} MiB. Время {stamp}; {age} с назад.")
            except (OSError, ValueError, KeyError, TypeError):
                print('Виджеты не запущены в доступной сессии; сохранённые измерения недоступны.')
    elif action == 'remove':
        remove()
    else:
        raise RuntimeError('Использование: desktop-widgets [start|stop|restart|toggle|status|remove]')


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError) as error:
        print(error, file=sys.stderr)
        sys.exit(1)
