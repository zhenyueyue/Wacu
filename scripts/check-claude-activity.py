"""Native lifecycle/configuration checks without launching Claude or calling an API."""
import concurrent.futures
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

repo = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--release', action='store_true')
args = parser.parse_args()
exe = repo / 'target' / ('release' if args.release else 'debug') / 'wacu.exe'
root = repo / 'target' / f'qa-claude-activity-{time.time_ns()}'
root.mkdir(parents=True)
events = root / 'events'
environment = os.environ.copy()
environment.pop('WACU_QUOTA_READER', None)
environment['CLAUDE_CONFIG_DIR'] = str(root / 'config')
environment['LOCALAPPDATA'] = str(root / 'appdata')

def run(arguments, data=None, env=None):
    result = subprocess.run([str(exe), *arguments], input=json.dumps(data) if data else None,
                            text=True, encoding='utf-8', capture_output=True,
                            env=env or environment, creationflags=subprocess.CREATE_NO_WINDOW,
                            timeout=10, check=True)
    return result.stdout.strip()

def signal(session, event, **extra):
    return run(['--claude-activity-hook', '--activity-dir', str(events)],
               {'session_id': session, 'hook_event_name': event, **extra})

def count():
    return int(run(['--claude-activity-status', '--activity-dir', str(events)]))

preferences = Path(os.environ['APPDATA']) / 'Wacu/preferences.json'
before = hashlib.sha256(preferences.read_bytes()).hexdigest() if preferences.exists() else None
assert count() == 0
signal('terminal', 'SessionStart')
assert count() == 0, 'An open CLI is not a working task'
signal('terminal', 'UserPromptSubmit', prompt='PRIVATE_FIXTURE_DO_NOT_STORE')
signal('desktop-code', 'UserPromptSubmit')
assert count() == 2
signal('terminal', 'Stop')
assert count() == 1, 'Another session must keep monitoring active'
signal('desktop-code', 'PermissionRequest')
assert count() == 0
signal('desktop-code', 'PostToolUse')
assert count() == 1
signal('desktop-code', 'SessionStart', source='compact')
assert count() == 1, 'Compaction must not mark a working task idle'
signal('desktop-code', 'Notification', notification_type='permission_prompt')
assert count() == 0
signal('desktop-code', 'UserPromptSubmit')
signal('desktop-code', 'SubagentStart', agent_id='background')
signal('desktop-code', 'Stop')
assert count() == 1
signal('desktop-code', 'PostToolUse', agent_id='background')
signal('desktop-code', 'SubagentStop', agent_id='background')
assert count() == 0
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
    list(pool.map(lambda i: signal('parallel', 'SubagentStart', agent_id=f'a{i}'), range(12)))
assert count() == 1
for i in range(11):
    signal('parallel', 'SubagentStop', agent_id=f'a{i}')
assert count() == 1, 'Parallel hook writes must not lose agents'
signal('parallel', 'SubagentStop', agent_id='a11')
assert count() == 0
reader = environment | {'WACU_QUOTA_READER': '1'}
run(['--claude-activity-hook', '--activity-dir', str(events)],
    {'session_id': 'quota-reader', 'hook_event_name': 'UserPromptSubmit'}, reader)
assert count() == 0, 'Quota reads must not activate the work monitor'
assert all('PRIVATE_FIXTURE_DO_NOT_STORE' not in path.read_text(encoding='utf-8')
           for path in events.glob('*.json'))
signal('terminal', 'SessionEnd')
signal('desktop-code', 'SessionEnd')
signal('parallel', 'SessionEnd')
assert not list(events.glob('*.json'))
print('PASS native work/idle/waiting, multiple sessions, compaction, parallel agents, quota-reader exclusion and privacy')

config = root / 'config/settings.json'
config.parent.mkdir(parents=True)
original = {'env': {'CUSTOM': 'fixture'}, 'permissions': {'allow': ['Read']},
            'hooks': {'Stop': [{'hooks': [{'type': 'command', 'command': 'existing-hook'}]}]}}
config.write_text(json.dumps(original), encoding='utf-8')
original_bytes = config.read_bytes()
run(['--install-claude-activity'])
installed = json.loads(config.read_text(encoding='utf-8'))
run(['--install-claude-activity'])
assert json.loads(config.read_text(encoding='utf-8')) == installed
assert installed['env'] == original['env'] and installed['permissions'] == original['permissions']
assert installed['hooks']['Stop'][0] == original['hooks']['Stop'][0]
assert all(sum('--claude-activity-hook' in h.get('args', []) for group in groups
               for h in group['hooks']) == 1 for groups in installed['hooks'].values())
assert all(path.read_bytes() == original_bytes or json.loads(path.read_text(encoding='utf-8')) == installed
           for path in config.parent.glob('settings.wacu-backup-*.json'))
run(['--uninstall-claude-activity'])
assert json.loads(config.read_text(encoding='utf-8')) == original
after = hashlib.sha256(preferences.read_bytes()).hexdigest() if preferences.exists() else None
assert before == after
print('PASS idempotent configuration merge, original hooks, exact backups, removal and untouched user preferences')
