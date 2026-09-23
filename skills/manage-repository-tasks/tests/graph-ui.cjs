// Exercise task-tree hierarchy and graph navigation without launching a browser.
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
  click() { this.events.click({ preventDefault() {} }); }
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
  const source = template.slice(template.indexOf('    const graphTasks ='), template.indexOf('    function taskSearchText('));
  vm.runInContext(source + '\nrenderGraphSupplement();', context);
  return { ids, navigated };
}

function descendants(element, predicate) {
  return element.children.flatMap((child) => [ ...(predicate(child) ? [child] : []), ...descendants(child, predicate) ]);
}
const buttons = (element) => descendants(element, (child) => child.tag === 'button');
const taskId = (button) => button.children[0].text;

test('example tasks contain no sensitive environment identifiers', () => {
  const taskDirectory = path.join(__dirname, 'tatviewer/tasks');
  const examplePaths = [
    path.join(__dirname, 'fixtures/access-workflow.json'),
    ...fs.readdirSync(taskDirectory)
      .filter((name) => name.endsWith('.md'))
      .map((name) => path.join(taskDirectory, name))
  ];
  const prohibited = [
    ['email address', /\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b/i],
    ['web URL', /https?:\/\//i],
    ['UNC path', /\\\\[A-Z0-9._$ -]+\\/i],
    ['absolute drive path', /\b[A-Z]:[\\/]/],
    ['drive identifier', /\b[A-Z]:/],
    ['private IP address', /\b(?:10|192\.168|172\.(?:1[6-9]|2\d|3[01]))(?:\.\d{1,3}){2}\b/],
    ['Git commit hash', /\b[0-9a-f]{7,40}\b/i],
    ['source-specific name', /\b(?:ERP|AppVer|AccessLowLevel|SysData|AppName|ImportExistingDbObjects|DB_objects_export)\b/i]
  ];
  for (const examplePath of examplePaths) {
    const content = fs.readFileSync(examplePath, 'utf8');
    for (const [label, pattern] of prohibited) {
      assert.doesNotMatch(content, pattern, `${path.basename(examplePath)} contains a ${label}`);
    }
  }
});

test('JSON and Markdown example tasks use the same titles', () => {
  const taskDirectory = path.join(__dirname, 'tatviewer/tasks');
  const filenamesById = new Map(
    fs.readdirSync(taskDirectory)
      .filter((name) => name.endsWith('.md'))
      .map((name) => [name.split(',')[0], name])
  );
  const titlesById = new Map(fixture.tasks.map((task) => [task.id, task.title]));
  assert.equal(filenamesById.size, fixture.tasks.length);
  for (const task of fixture.tasks) {
    assert.equal(filenamesById.get(task.id), task.filename);
    const relationships = task.relationships || {};
    const references = [
      ...(relationships.parent ? [relationships.parent] : []),
      ...(relationships.children || []),
      ...(relationships.blocks || []),
      ...(relationships.blocked_by || [])
    ];
    for (const reference of references) {
      assert.equal(reference.title, titlesById.get(reference.id), `Stale title for ${reference.id}`);
    }
  }
});

test('search indexes only a task own fields', () => {
  const task = {
    id: 't@self',
    title: 'Own title',
    body_markdown: 'Own details',
    type: 'TASK',
    status: 'ready',
    priority_text: '4',
    relationships: { parent: { id: 't@parent', title: 'Relationship-only phrase' } },
    blocking_causes: [{ blocker: { title: 'Blocking-only phrase' } }]
  };
  const context = vm.createContext({ tasks: [task], searchIndex: new Map() });
  const source = template.slice(template.indexOf('    function taskSearchText('), template.indexOf('    function domId('));
  vm.runInContext(source, context);
  assert.match(context.searchIndex.get(task.id), /own title own details task ready 4/);
  assert.doesNotMatch(context.searchIndex.get(task.id), /relationship-only|blocking-only/);
});

function cardBadges(task) {
  const context = vm.createContext({ makeElement: (tag, cls, text) => new Element(tag, cls, text) });
  const source = template.slice(template.indexOf('    function createBadge('), template.indexOf('    function uniqueReferences('));
  vm.runInContext(source, context);
  return context.createCardBadges(task).children.map((badge) => badge.text);
}

test('active cards move status out of the headline badges and into the left rail', () => {
  const task = { type: 'FEATURE', priority_text: '2' };
  assert.deepEqual(cardBadges({ ...task, status: 'blocked' }), ['FEATURE', 'P2']);
  assert.deepEqual(cardBadges({ ...task, status: 'ready' }), ['FEATURE', 'P2']);
  assert.deepEqual(cardBadges({ ...task, status: 'done' }), ['done', 'FEATURE', 'P2']);

  const context = vm.createContext({ makeElement: (tag, cls, text) => new Element(tag, cls, text) });
  const source = template.slice(template.indexOf('    function createBadge('), template.indexOf('    function uniqueReferences('));
  vm.runInContext(source, context);
  assert.equal(context.createCardStatusRail({ status: 'ready' }).className, 'card-status-rail card-status-rail-ready');
  assert.equal(context.createCardStatusRail({ status: 'ready' }).text, 'ready');
  assert.equal(context.createCardStatusRail({ status: 'blocked' }).className, 'card-status-rail card-status-rail-blocked');
  assert.equal(context.createCardStatusRail({ status: 'ready' }, true).text, 'R');
  assert.equal(context.createCardStatusRail({ status: 'blocked' }, true).text, 'B');
  assert.equal(context.createCardStatusRail({ status: 'done' }), null);
  assert.equal(context.createCardBadges({ ...task, status: 'ready' }).children[0].className, 'badge badge-type-feature');
  assert.equal(context.createCardBadges({ ...task, type: 'BUG', status: 'ready' }).children[0].className, 'badge badge-type-bug');
  assert.equal(context.createCardBadges({ ...task, type: 'TASK', status: 'ready' }).children[0].className, 'badge badge-type-task');
});

test('BLOCKED BY and BLOCKS use matching blocked labels with colons', () => {
  const context = vm.createContext({ makeElement: (tag, cls, text) => new Element(tag, cls, text) });
  const source = template.slice(template.indexOf('    function createBadge('), template.indexOf('    function uniqueReferences('));
  vm.runInContext(source, context);
  for (const label of ['BLOCKED BY', 'BLOCKS']) {
    const element = context.createRelationshipLabel(label);
    assert.equal(element.className, 'badge badge-blocked');
    assert.equal(element.text, `${label}:`);
  }
  assert.equal(context.createRelationshipLabel('CHILDREN').className, 'relation-inline-label');
});

test('BLOCKS references omit a redundant blocked status pill', () => {
  const context = vm.createContext({
    makeElement: (tag, cls, text) => new Element(tag, cls, text),
    domId: (id) => id,
    createCopyButton: () => new Element('button', 'copy-button'),
    navigateToTask: () => {},
    taskById: new Map(),
    attachHoverPreview: () => {}
  });
  const source = template.slice(template.indexOf('    function createTaskReference('), template.indexOf('    function createDescription('));
  vm.runInContext(source, context);
  const blocked = { id: 't@next', title: 'Next task', status: 'blocked', priority_text: '2' };
  const section = context.createCompactRelationships({ relationships: { blocks: [blocked] }, blocking_causes: [] });
  assert.equal(descendants(section, (element) => element.className === 'ref-status ref-status-blocked').length, 0);
  assert.equal(descendants(section, (element) => element.className === 'ref-priority')[0].text, 'P2');
});

test('BLOCKED BY uses bold status-colored titles without status pills', () => {
  const context = vm.createContext({
    makeElement: (tag, cls, text) => new Element(tag, cls, text),
    domId: (id) => id,
    createCopyButton: () => new Element('button', 'copy-button'),
    navigateToTask: () => {},
    taskById: new Map(),
    attachHoverPreview: () => {}
  });
  const source = template.slice(template.indexOf('    function createTaskReference('), template.indexOf('    function createDescription('));
  vm.runInContext(source, context);
  const blocker = { id: 't@first', title: 'Required task', status: 'ready', priority_text: '1' };
  const secondBlocker = { id: 't@second', title: 'Blocked prerequisite', status: 'blocked', priority_text: '3' };
  const blocked = { id: 't@next', title: 'Next task', status: 'blocked', priority_text: '2' };
  const task = {
    relationships: { blocked_by: [blocker, secondBlocker], blocks: [blocked], children: [] },
    blocking_causes: []
  };
  const upper = context.createBlockedByRelationship(task);
  const lower = context.createCompactRelationships(task);
  assert.deepEqual(
    descendants(upper, (element) => element.className === 'badge badge-blocked').map((element) => element.text),
    ['BLOCKED BY:']
  );
  assert.deepEqual(
    descendants(lower, (element) => element.className === 'badge badge-blocked').map((element) => element.text),
    ['BLOCKS:']
  );
  assert.equal(descendants(upper, (element) => element.className.startsWith('ref-status')).length, 0);
  assert.deepEqual(
    descendants(upper, (element) => element.className.includes('ref-title-status')).map((element) => [element.className, element.text]),
    [
      ['ref-title ref-title-status ref-title-ready', 'Required task'],
      ['ref-title ref-title-status ref-title-blocked', 'Blocked prerequisite']
    ]
  );
  assert.deepEqual(
    descendants(upper, (element) => element.className === 'ref-priority').map((element) => element.text),
    ['P1', 'P3']
  );
  assert.equal(context.createCompactRelationships({ relationships: { children: [blocker] }, blocking_causes: [] }), null);
});

test('task cards place BLOCKED BY between the title and details', () => {
  const context = vm.createContext({
    makeElement: (tag, cls, text) => new Element(tag, cls, text),
    domId: (id) => id,
    createTaskReference: () => new Element('span', 'task-ref'),
    createCardBadges: () => new Element('div', 'badges'),
    createCardStatusRail: (task, compact) => new Element('span', 'card-status-rail card-status-rail-blocked', compact ? 'B' : 'blocked'),
    createDescription: () => new Element('button', 'description-toggle'),
    createBlockedByRelationship: () => new Element('section', 'relationships-compact blocked-by-relationship'),
    createDetails: () => new Element('section', 'task-details'),
    createCompactRelationships: () => new Element('section', 'relationships-compact')
  });
  const source = template.slice(template.indexOf('    function createTaskCard('), template.indexOf('    function createTreeParentLink('));
  vm.runInContext(source, context);
  const card = context.createTaskCard({ id: 't@child', status: 'blocked' });
  assert.deepEqual(card.children[0].children.map((element) => element.className), [
    'card-status-rail card-status-rail-blocked',
    'card-topline',
    'relationships-compact blocked-by-relationship',
    'task-details',
    'relationships-compact'
  ]);
  const contextCard = context.createTaskCard({ id: 't@parent', status: 'blocked' }, true);
  assert.equal(contextCard.className, 'task-card task-card-context');
  assert.equal(contextCard.dataset.context, 'ancestor');
  assert.deepEqual(contextCard.children[0].children.map((element) => [element.className, element.text]), [
    ['card-status-rail card-status-rail-blocked', 'B'],
    ['card-topline', '']
  ]);
});

function taskTree(matchingTasks, allTasks = matchingTasks) {
  const navigated = [];
  const context = vm.createContext({
    tasks: allTasks,
    taskById: new Map(allTasks.map((task) => [task.id, task])),
    makeElement: (tag, cls, text) => new Element(tag, cls, text),
    domId: (id) => id,
    navigateToTask: (...args) => navigated.push(args),
    createTaskCard: (task, contextOnly) => {
      const card = new Element('li', contextOnly ? 'task-card task-card-context' : 'task-card');
      card.dataset.taskId = task.id;
      return card;
    }
  });
  const source = template.slice(template.indexOf('    function createTreeParentLink('), template.indexOf('    function matchesFilters('));
  vm.runInContext(source, context);
  const treeView = context.createTreeView(matchingTasks);
  return {
    roots: context.createTaskTreeItems(treeView.visibleTasks, treeView.contextIds),
    contextIds: treeView.contextIds,
    navigated
  };
}

test('the issue tree includes each sample task once and links connector lines to parents', () => {
  const { roots, navigated } = taskTree(fixture.tasks);
  const actual = new Map();
  function visit(items, parent = null) {
    for (const item of items) {
      const id = item.dataset.taskId;
      assert.ok(!actual.has(id), `Duplicate task ${id}`);
      actual.set(id, parent);
      const parentLink = item.children.find((child) => child.className === 'task-tree-parent-link');
      if (parent) {
        assert.ok(parentLink, `Missing connector link from ${id} to ${parent}`);
        assert.equal(parentLink.href, `#${parent}`);
        parentLink.click();
        assert.deepEqual(navigated.at(-1), [parent, true]);
      } else {
        assert.equal(parentLink, undefined, `Root task ${id} has a parent connector link`);
      }
      const children = item.children.find((child) => child.className === 'task-tree-children');
      if (children) visit(children.children, id);
    }
  }
  visit(roots);
  assert.equal(actual.size, 17);
  for (const task of fixture.tasks) assert.equal(actual.get(task.id), task.relationships.parent?.id || null);
});

test('a matching descendant keeps its complete dimmed ancestor chain', () => {
  const child = structuredClone(fixture.tasks.find((task) => task.id === 't@rtio'));
  assert.equal(child.relationships.parent.id, 't@goz4');
  const { roots, contextIds } = taskTree([child], fixture.tasks);
  assert.deepEqual(Array.from(contextIds), ['t@goz4', 't@m46p']);
  assert.deepEqual(Array.from(roots, (item) => item.dataset.taskId), ['t@m46p']);
  assert.equal(roots[0].className, 'task-card task-card-context');
  const children = roots[0].children.find((element) => element.className === 'task-tree-children');
  assert.deepEqual(Array.from(children.children, (item) => [item.dataset.taskId, item.className]), [
    ['t@goz4', 'task-card task-card-context']
  ]);
  const grandchildren = children.children[0].children.find((element) => element.className === 'task-tree-children');
  assert.deepEqual(Array.from(grandchildren.children, (item) => [item.dataset.taskId, item.className]), [
    ['t@rtio', 'task-card']
  ]);
});

test('the tree preserves repository order for roots and siblings', () => {
  const rootLaterPriority = { id: 't@root-z', priority: 9, relationships: {} };
  const childSecond = { id: 't@child-z', priority: 9, relationships: { parent: { id: 't@root-z' } } };
  const childFirst = { id: 't@child-a', priority: 0, relationships: { parent: { id: 't@root-z' } } };
  const rootEarlierPriority = { id: 't@root-a', priority: 0, relationships: {} };
  const ordered = [rootLaterPriority, childSecond, childFirst, rootEarlierPriority];
  const { roots } = taskTree(ordered);
  assert.deepEqual(Array.from(roots, (item) => item.dataset.taskId), ['t@root-z', 't@root-a']);
  const children = roots[0].children.find((element) => element.className === 'task-tree-children');
  assert.deepEqual(Array.from(children.children, (item) => item.dataset.taskId), ['t@child-z', 't@child-a']);
});

test('unconnected graph tasks remain available and open the correct card', () => {
  const { ids, navigated } = setup();
  const unlinked = buttons(ids.get('graph-unlinked'));
  assert.deepEqual(unlinked.map(taskId).sort(), ['t@2cio', 't@goz4', 't@m46p', 't@pyph', 't@q3b3', 't@ypov']);
  for (const button of unlinked) {
    button.click();
    assert.deepEqual(navigated.at(-1), [taskId(button), true]);
  }
});

test('the redundant Task families graph view is absent', () => {
  assert.doesNotMatch(template, /graph-families|Task families|renderGraphFamilies/);
});

test('empty reports and graph supplements remain usable', () => {
  assert.equal(taskTree([]).roots.length, 0);
  assert.equal(buttons(setup([]).ids.get('graph-unlinked')).length, 0);
  const child = structuredClone(fixture.tasks.find((task) => task.id === 't@2cio'));
  const { ids } = setup([child]);
  assert.deepEqual(buttons(ids.get('graph-unlinked')).map(taskId), ['t@2cio']);
});
