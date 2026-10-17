#!/usr/bin/env python3
"""Local test-VM controls. Not a game/server admin tool.

UTM's sandbox disallows custom Unix socket binding on external volumes.
The temporary QA endpoint binds ONLY 127.0.0.1. Remove it after installation.
"""
import argparse
import json
import socket
import time

parser = argparse.ArgumentParser()
parser.add_argument('action', choices=['status', 'key', 'type', 'click', 'screenshot'])
parser.add_argument('value', nargs='?')
parser.add_argument('--socket', help='Optional Unix socket instead of loopback QA endpoint')
args = parser.parse_args()
sock = socket.socket(socket.AF_UNIX if args.socket else socket.AF_INET, socket.SOCK_STREAM)
sock.settimeout(10)
sock.connect(args.socket or ('127.0.0.1', 4445))
stream = sock.makefile('rwb', buffering=0)
json.loads(stream.readline())

def command(name, arguments=None):
    stream.write((json.dumps({'execute': name, 'arguments': arguments or {}}) + '\n').encode())
    while True:
        result = json.loads(stream.readline())
        if 'error' in result:
            raise RuntimeError(result['error'])
        if 'return' in result:
            return result['return']

def key(keys):
    command('send-key', {'keys': [{'type': 'qcode', 'data': k} for k in keys.split('+')], 'hold-time': 100})
    time.sleep(0.2)

command('qmp_capabilities')
if args.action == 'status':
    print(command('query-status'))
elif args.action == 'key':
    key(args.value)
elif args.action == 'type':
    mapping = {' ': 'spc', '\\': 'backslash', '/': 'slash', ':': 'shift+semicolon', '.': 'dot', '_': 'shift+minus', '-': 'minus', '\n': 'ret', '"': 'shift+apostrophe', "'": 'apostrophe', ';': 'semicolon', '=': 'equal', '$': 'shift+4', '|': 'shift+backslash', '(': 'shift+9', ')': 'shift+0', ',': 'comma', '{': 'shift+bracket_left', '}': 'shift+bracket_right', '[': 'bracket_left', ']': 'bracket_right', '>': 'shift+dot', '<': 'shift+comma', '&': 'shift+7', '?': 'shift+slash', '!': 'shift+1'}
    for char in args.value:
        key(mapping.get(char, ('shift+' + char.lower()) if char.isupper() else char))
elif args.action == 'click':
    x, y, width, height = map(int, args.value.split(','))
    assert 0 <= x < width and 0 <= y < height
    command('input-send-event', {'events': [
        {'type': 'abs', 'data': {'axis': 'x', 'value': round(x * 32767 / (width - 1))}},
        {'type': 'abs', 'data': {'axis': 'y', 'value': round(y * 32767 / (height - 1))}},
        {'type': 'btn', 'data': {'button': 'left', 'down': True}},
    ]})
    time.sleep(0.1)
    command('input-send-event', {'events': [{'type': 'btn', 'data': {'button': 'left', 'down': False}}]})
elif args.action == 'screenshot':
    # UTM's QEMU lacks libpng support; write PPM and convert with macOS sips.
    print(command('screendump', {'filename': args.value}))
