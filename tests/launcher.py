"""The editable launcher must stop its local model when it is terminated."""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

with tempfile.TemporaryDirectory() as directory:
    runtime = Path(directory) / 'runtime'
    (runtime / 'llama').mkdir(parents=True)
    shutil.copy('runtime/start.sh', runtime / 'start.sh')
    for path in [runtime / 'kron', runtime / 'llama' / 'llama-server']:
        path.write_text('#!/bin/sh\necho $$ > "' + str(path) + '.pid"\nexec sleep 300\n')
        path.chmod(0o700)
    process = subprocess.Popen(['sh', str(runtime / 'start.sh')], start_new_session=True)
    try:
        for _ in range(100):
            if all(Path(str(p) + '.pid').exists() for p in [runtime / 'kron', runtime / 'llama' / 'llama-server']):
                break
            time.sleep(.05)
        else:
            raise AssertionError('Launcher did not start both children')
        children = [int(p.read_text()) for p in runtime.rglob('*.pid')]
        assert int((runtime / 'pid').read_text()) == process.pid
        process.terminate()
        process.wait(timeout=5)
        for child in children:
            for _ in range(100):
                try:
                    os.kill(child, 0)
                    stat = Path(f'/proc/{child}/stat')
                    if stat.exists() and stat.read_text().split()[2] == 'Z':
                        break
                except ProcessLookupError:
                    break
                time.sleep(.05)
            else:
                raise AssertionError(f'Orphan child: {child}')
        print('PASS: launcher starts and stops model and agent together')
    finally:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
