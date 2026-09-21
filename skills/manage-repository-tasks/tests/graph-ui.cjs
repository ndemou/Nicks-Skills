// Exercise graph navigation and hierarchy without launching a browser.
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const template = fs.readFileSync(path.join(__dirname, '../src/tatviewer.html'), 'utf8');
const fixture = JSON.parse(fs.readFileSync(path.join(__dirname, 'fixtures/access-workflow.json'), 'utf8'));

class Element {
  constructor(tag, className = '', text = '') {
    Object.assign(this, { tag, className, text, children: [], dataset: {}, events: {}, attributes: {}, hidden: false });
  }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.children = children; }
  setAttribute(key, value) { this.attributes[key] = value; }
  addEventListener(event, callback) { this.events[event] = callback; }
  click() { this.events.click(); }
}

function setup(tasks = fixture.tasks) {
  const ids = new Map([...template.matchAll(/\bid="([^"]+)"/g)].map((match) => [match[1], new Element('div')]));
  const navigated = [];
  const context = vm.createContext({
    tasks,
    taskStatusBucket: (task) => task.status === 'done' && !task.completed_at ? 'old-done' : task.status,
    makeElement: (tag, cls, text) => new Element(tag, cls, text),
    navigateToTask: (...args) => navigated.push(args),
    document: {
      getElementById: (id) => { assert.ok(ids.has(id), `Missing element ${id}`); return ids.get(id); },
      createTextNode: (text) => new Element('#text', '', text)
    }
  });
  const source = template.slice(template.indexOf('    const graphTasks ='), template.indexOf('    function directReferences('));
  vm.runInContext(source + '\nrenderGraphFamilies();', context);
  return { ids, navigated };
}

function descendants(element, predicate) {
  return element.children.flatMap((child) => [ ...(predicate(child) ? [child] : []), ...descendants(child, predicate) ]);
}
const buttons = (element) => descendants(element, (child) => child.tag === 'button');
const taskId = (button) => button.children[0].text;

test('the family tree includes each sample task once, under its exact parent', () => {
  const { ids } = setup();
  const family = ids.get('graph-families');
  assert.equal(buttons(family).length, 17);
  const actual = new Map();
  function visit(list, parent = null) {
    for (const item of list.children) {
      const id = taskId(item.children[0]);
      assert.ok(!actual.has(id), `Duplicate task ${id}`);
      actual.set(id, parent);
      if (item.children[1]) visit(item.children[1], id);
    }
  }
  visit(family.children[0]);
  for (const task of fixture.tasks) assert.equal(actual.get(task.id), task.relationships.parent?.id || null);
});

test('unconnected tasks remain available, and every reference opens the correct card', () => {
  const { ids, navigated } = setup();
  const unlinked = buttons(ids.get('graph-unlinked'));
  assert.deepEqual(unlinked.map(taskId).sort(), ['t@2cio', 't@goz4', 't@m46p', 't@pyph', 't@q3b3', 't@ypov']);
  for (const button of [...buttons(ids.get('graph-families')), ...unlinked]) {
    button.click();
    assert.deepEqual(navigated.at(-1), [taskId(button), true]);
  }
});

test('the graph view controls switch content and pressed state in both directions', () => {
  const { ids } = setup();
  for (const selected of ['families', 'flow']) {
    ids.get(`graph-${selected}-button`).click();
    for (const view of ['families', 'flow']) {
      assert.equal(ids.get(`graph-${view}`).hidden, view !== selected);
      assert.equal(ids.get(`graph-${view}-button`).attributes['aria-pressed'], String(view === selected));
    }
  }
});

test('empty reports and a visible child of an excluded parent remain usable', () => {
  assert.equal(buttons(setup([]).ids.get('graph-families')).length, 0);
  const parent = structuredClone(fixture.tasks.find((task) => task.id === 't@m46p'));
  parent.status = 'done';
  parent.completed_at = null;
  const child = structuredClone(fixture.tasks.find((task) => task.id === 't@2cio'));
  const { ids } = setup([parent, child]);
  assert.deepEqual(buttons(ids.get('graph-families')).map(taskId), ['t@2cio']);
  assert.deepEqual(buttons(ids.get('graph-unlinked')).map(taskId), ['t@2cio']);
});
