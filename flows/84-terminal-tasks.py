"""The task view in the real terminal, backed by the server's task files and live stream."""
from claude import reply
from driver import flow
from model import tool
from terminal import Terminal
import server

seed, close = 'SEED_TASKS_84', 'CLOSE_TASK_84'
replies = [*(tool(seed, 'task', action='write', title=f'Task view item {n}',
                  text=f'Description of item {n}', **({'claim': True} if n == 1 else {'due': '2020-11-06', 'priority': 3} if n == 3 else {}))
             for n in range(1, 19)),
           reply(text='Tasks seeded', match=seed),
           tool(close, 'task', action='write', id=1, close='Finished in this session'),
           reply(text='Task closed', match=close)]
with flow(replies) as f:
    f.base()
    session = f.ask(seed)['session']
    with Terminal(f, '--resume', session, rows=50, columns=160) as t:
        t.shown('Tasks seeded', 'Message the agent')
        t.shown('TASKS', '● 1  ', '○ 2  ')
        pane = '\n'.join(line[120:] for line in t.screen.display)
        assert pane.index('● 1  ') < pane.index('○ 3  ') < pane.index('○ 2  '), pane
        assert 'overdue Nov 6' in pane, pane
        t.click('more')
        t.shown('fewer', 'Task view item 15')
        t.click('more')
        t.shown('Task view item 18')
        t.click('newer')
        t.shown('fewer')
        t.click('fewer')
        t.send('/tasks\r')
        t.shown('READY', 'Task view item 1')
        t.send('\x1b')
        t.until(lambda t: '── READY' not in t.text())
        t.send('/tasks 1\r')
        t.shown('── DESCRIPTION', 'Description of item 1')
        t.send('\x1b')
        t.until(lambda t: '── DESCRIPTION' not in t.text())
        t.shown('READY', 'Task view item 1')
        t.send('\x1b')
        server.post(f, f'/sessions/{session}/input', {'text': close})
        t.shown('✓ 1 done this session')
    empty = f.home / 'empty'
    empty.mkdir()
    other = server.post(f, '/sessions', {'cwd': str(empty)})['session']
    with Terminal(f, '--resume', other, rows=50, columns=160) as t:
        t.shown('Message the agent')
        t.pump(1)
        assert 'TASKS' not in t.text(), t.text()
