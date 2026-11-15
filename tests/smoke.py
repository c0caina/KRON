"""Exercise HTTP, the model/tool exchange, and cross-origin rejection without an API key."""
import http.server
import json
import os
from pathlib import Path
import socket
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

calls = []

class Model(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        assert self.path == '/v1/chat/completions'
        assert [t['function']['name'] for t in body['tools']] == ['console']
        calls.append(body)
        if body['messages'][-1]['role'] == 'user':
            reply = {'role': 'assistant', 'content': None, 'tool_calls': [{
                'id': 'test', 'type': 'function', 'function': {
                    'name': 'console', 'arguments': json.dumps({'command': 'printf kron-smoke'})}}]}
        else:
            assert 'stdout:\nkron-smoke' in body['messages'][-1]['content']
            assert body['messages'][-1]['tool_call_id'] == 'test'
            reply = {'role': 'assistant', 'content': 'done'}
        data = json.dumps({'choices': [{'message': reply}]}).encode()
        self.send_response(200)
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

server = http.server.HTTPServer(('127.0.0.1', 0), Model)
threading.Thread(target=server.serve_forever, daemon=True).start()
with socket.socket() as sock:
    sock.bind(('127.0.0.1', 0))
    port = sock.getsockname()[1]
origin = f'http://127.0.0.1:{port}'
with tempfile.TemporaryDirectory() as directory:
    runtime = Path(directory) / 'runtime'
    shutil.copytree('runtime', runtime)
    config = Path(directory) / 'kron.json'
    config.write_text(json.dumps({'base_url': f'http://127.0.0.1:{server.server_port}/v1', 'model': 'mock'}))
    binary = Path(os.environ.get('KRON_BIN', 'target/debug/kron')).resolve()
    process = subprocess.Popen([str(binary)], cwd=directory, env={**os.environ, 'KRON_CONFIG': str(config), 'KRON_LISTEN': f'127.0.0.1:{port}', 'KRON_RUNTIME': str(runtime)})
    try:
        for _ in range(100):
            try:
                page = urllib.request.urlopen(origin).read()
                break
            except urllib.error.URLError:
                if process.poll() is not None:
                    raise RuntimeError('Harness exited')
                time.sleep(.05)
        else:
            raise RuntimeError('Harness did not start')
        assert b'<textarea' in page
        def send(source):
            return urllib.request.urlopen(urllib.request.Request(origin + '/message',
                data=b'{"text":"test"}', headers={'Content-Type': 'application/json', 'X-Kron': '1', 'Origin': source}))
        try:
            send('https://unrelated.example')
            raise AssertionError('Foreign origin accepted')
        except urllib.error.HTTPError as error:
            assert error.code == 403
        assert not calls
        assert json.load(send(origin)) == {'text': 'done'}
        (runtime / 'system.md').write_text('edited system prompt')
        (runtime / 'ui' / 'index.html').write_text('<button>edited UI</button>')
        (runtime / 'ui' / 'app.js').write_text('window.edited=true;')
        assert urllib.request.urlopen(origin).read() == b'<button>edited UI</button>'
        assert b'window.edited' in urllib.request.urlopen(origin + '/app.js').read()
        config.write_text(json.dumps({'base_url': f'http://127.0.0.1:{server.server_port}/v1', 'model': 'edited-model'}))
        assert json.load(send(origin)) == {'text': 'done'}
        assert calls[2]['messages'][0]['content'] == 'edited system prompt'
        assert calls[2]['model'] == 'edited-model'
        assert len(calls) == 4
        assert len(calls[2]['messages']) == 6  # Previous tool exchange retained.
        print('PASS: local UI, origin check, console round trip, conversation context')
    finally:
        process.terminate()
        process.wait(timeout=10)
server.shutdown()
