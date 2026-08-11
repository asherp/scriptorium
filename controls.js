// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The tuning panel.
//
// Every knob here binds straight to the `params` object the engine hands out
// as its own defaults and takes back with every call, so there is exactly one
// definition anywhere of what a knob starts at.
//
// Knobs come in two kinds and they clear differently. A GRAMMAR knob shapes
// the symbolic derivation, which is cached forever per source: change one and
// the cache has to be dropped, or a seed keeps answering with generations
// grown under the old rules. A TURTLE knob only affects the geometric walk,
// which is never cached, so a plain re-layout is enough. The `kind` on each
// group below is what tells the host which of the two just happened.

export const PARAM_GROUPS = [
  {
    title: 'Grammar',
    kind: 'grammar',
    note: 'Shapes the symbol string itself. Changing one re-derives every seed.',
    knobs: [
      ['branchinessPerGen', 'branchiness / gen', 0, 0.6, 0.005, 'How fast the branchier rules unlock, per generation.'],
      ['branchinessCap', 'branchiness cap', 0, 1, 0.01, "The ramp's ceiling."],
      ['branchinessForkThreshold', 'fork threshold', 0, 1, 0.01, 'Branchiness a reserved rule waits for.'],
      ['maxSizeBoost', 'max size boost', 0, 10, 1, 'Ceiling on the extra generations a large mark earns.'],
    ],
  },
  {
    title: 'Turtle',
    kind: 'turtle',
    note: 'Where the same string lands. Recomputed on every layout, never cached.',
    knobs: [
      ['step', 'step (px)', 1, 30, 0.5, 'Pixels per F.'],
      ['turnDeg', 'turn (°)', 0, 90, 1, 'Base turn angle for + and -.'],
      ['jitterDeg', 'jitter (°)', 0, 60, 1, "Random wobble on every F's heading."],
      ['maxDeflectTries', 'deflect tries', 0, 20, 1, 'How hard a blocked step dodges before giving up.'],
      ['leafDedupPx', 'leaf dedup (px)', 0, 20, 0.5, 'Leaves closer than this to one already placed collapse.'],
      ['obstaclePad', 'obstacle halo (px)', 0, 20, 0.5, 'Padding around every term box.'],
      ['boundsInset', 'page inset (px)', 0, 40, 0.5, "How far inside the page's edge growth stops."],
      ['overflow', 'overflow (px)', 0, 200, 1, 'Bound used when there is no page to ask.'],
      ['maxReachFloor', 'leash floor (px)', 0, 1200, 5, "The leash's minimum radius. A mark riding a block overrides it upward to that block's own reach."],
      ['maxReachMul', 'leash × size', 0, 8, 0.1, "The leash's radius per px of the mark's own size."],
      ['hullMargin', 'border margin', 0, 60, 0.5, "How far outside the halo a block's silhouette is ruled. It has to clear the line's leading, not just the ink."],
    ],
  },
  {
    title: 'Riding a rail',
    kind: 'turtle',
    note: 'How a vine runs the line it grew from — a letterform, or a block\'s silhouette — and how it leaves it. The ride\'s LENGTH is a grammar question: see the trunk count in the readout.',
    knobs: [
      ['glyphFollowMax', 'follow steps', 0, 600, 1, 'A cap on the opening run, not its length. It does nothing until the grammar\'s trunk outgrows it — to run more of a block, give the productions more trunk.'],
      ['glyphClearanceMul', 'clearance × step', 0, 3, 0.05, 'How near its own trail counts as crossing itself.'],
      ['glyphDepartDeg', 'depart (°)', 0, 180, 1, 'How sharply the vine turns away when it leaves.'],
      ['departForkSteps', 'depart fork steps', 0, 20, 1, 'Length of the shoot left behind at the departure. 0 disables it.'],
    ],
  },
];

/**
 * Builds one group's rows. `onChange(kind)` fires on every edit; the caller
 * decides whether that means resetting derivations or merely re-laying out.
 */
export function buildParamGroups(root, params, onChange) {
  root.textContent = '';
  const sync = [];
  for (const group of PARAM_GROUPS) {
    const box = document.createElement('details');
    box.open = true;
    box.className = 'group';
    const summary = document.createElement('summary');
    summary.innerHTML = `${group.title} <span class="kind kind-${group.kind}">${group.kind}</span>`;
    box.append(summary);
    if (group.note) {
      const note = document.createElement('p');
      note.className = 'note';
      note.textContent = group.note;
      box.append(note);
    }
    for (const [key, label, min, max, step, hint] of group.knobs) {
      const row = document.createElement('div');
      row.className = 'knob';
      row.innerHTML = `
        <label title="${hint}"><span>${label}</span></label>
        <input type="range" min="${min}" max="${max}" step="${step}">
        <input type="number" min="${min}" max="${max}" step="${step}" class="num">`;
      const [range, num] = row.querySelectorAll('input');
      const write = (v) => {
        const n = Number(v);
        if (!Number.isFinite(n)) return;
        params[key] = n;
        range.value = String(n);
        num.value = String(n);
        onChange(group.kind);
      };
      range.addEventListener('input', () => write(range.value));
      num.addEventListener('input', () => write(num.value));
      sync.push(() => {
        range.value = String(params[key]);
        num.value = String(params[key]);
      });
      box.append(row);
    }
    root.append(box);
  }
  const refresh = () => sync.forEach((f) => f());
  refresh();
  return refresh;
}

/**
 * The rule table. Every production keeps at least one F in the reference set,
 * which is what makes a later generation an elaboration of an earlier one
 * rather than a fresh roll — the editor says so rather than enforcing it, so
 * the rule can be broken on purpose to see what breaking it costs.
 */
export function buildProductions(root, params, onChange) {
  const render = () => {
    root.textContent = '';
    const table = document.createElement('div');
    table.className = 'productions';
    const head = document.createElement('div');
    head.className = 'prod-row prod-head';
    head.innerHTML = '<span>weight</span><span>F →</span><span title="Held back until branchiness clears the fork threshold">held</span><span></span>';
    table.append(head);

    params.productions.forEach((p, i) => {
      const row = document.createElement('div');
      row.className = 'prod-row';
      row.innerHTML = `
        <input type="number" class="num" min="0" step="0.5" value="${p.weight}">
        <input type="text" class="rule" value="${escapeAttr(p.to)}" spellcheck="false">
        <input type="checkbox" ${p.reserved ? 'checked' : ''}>
        <button class="ghost" title="Remove this rule">×</button>`;
      const [weight, to, reserved] = row.querySelectorAll('input');
      const remove = row.querySelector('button');
      weight.addEventListener('input', () => {
        p.weight = Number(weight.value) || 0;
        onChange();
      });
      to.addEventListener('input', () => {
        p.to = to.value;
        row.classList.toggle('barren', !to.value.includes('F'));
        onChange();
      });
      reserved.addEventListener('change', () => {
        p.reserved = reserved.checked;
        onChange();
      });
      remove.addEventListener('click', () => {
        params.productions.splice(i, 1);
        render();
        onChange();
      });
      row.classList.toggle('barren', !p.to.includes('F'));
      table.append(row);
    });

    const add = document.createElement('button');
    add.className = 'wide';
    add.textContent = '+ rule';
    add.addEventListener('click', () => {
      params.productions.push({ weight: 1, to: 'F[+F]F', reserved: false });
      render();
      onChange();
    });
    root.append(table, add);
  };
  render();
  return render;
}

/**
 * The stage ladder: which depth counts read as which stage, and how many
 * rewritings each one is worth. Buckets rather than a smooth curve, because a
 * stage is a cache key.
 */
export function buildStages(root, state, onChange) {
  const render = () => {
    root.textContent = '';
    const table = document.createElement('div');
    table.className = 'stages';
    const head = document.createElement('div');
    head.className = 'stage-row stage-head';
    head.innerHTML = '<span>from</span><span>to</span><span>name</span><span>iters</span>';
    table.append(head);
    state.stages.forEach((s, i) => {
      const row = document.createElement('div');
      row.className = 'stage-row';
      row.dataset.index = String(i);
      row.innerHTML = `
        <input type="number" class="num" step="1" value="${s.min}">
        <input type="text" class="num" value="${Number.isFinite(s.max) ? s.max : '∞'}">
        <input type="text" value="${escapeAttr(s.name)}" spellcheck="false">
        <input type="number" class="num" min="0" max="12" step="1" value="${s.iterations}">`;
      const [min, max, name, iters] = row.querySelectorAll('input');
      min.addEventListener('input', () => {
        s.min = Number(min.value) || 0;
        onChange();
      });
      max.addEventListener('input', () => {
        s.max = /^[∞]|inf/i.test(max.value) ? Infinity : Number(max.value) || 0;
        onChange();
      });
      name.addEventListener('input', () => {
        s.name = name.value;
        onChange();
      });
      iters.addEventListener('input', () => {
        s.iterations = Math.max(0, Number(iters.value) || 0);
        onChange();
      });
      table.append(row);
    });
    root.append(table);
  };
  render();
  return render;
}

/**
 * The notation: which marks this page grows from.
 *
 * Every opcode is on by default and any of them can be switched off, one at a
 * time or a whole group at once. Nothing here is the engine's business — it
 * only ever sees the rectangles the marks the host settled on were measured
 * at — which is exactly why it can be a list a reader edits.
 */
export function buildNotation(root, notation, state, onChange) {
  root.textContent = '';

  const bar = document.createElement('div');
  bar.className = 'note-bar';
  for (const [label, off] of [['all', false], ['none', true]]) {
    const b = document.createElement('button');
    b.textContent = label;
    b.addEventListener('click', () => {
      state.disabledOps.clear();
      if (off) for (const g of notation) for (const [, name] of g.ops) state.disabledOps.add(name);
      render();
      onChange();
    });
    bar.append(b);
  }
  root.append(bar);

  const groups = [];
  const render = () => {
    for (const { box, chips, ops } of groups) {
      const on = ops.filter(([, name]) => !state.disabledOps.has(name)).length;
      box.querySelector('.count').textContent = `${on}/${ops.length}`;
      box.querySelector('input').checked = on > 0;
      box.querySelector('input').indeterminate = on > 0 && on < ops.length;
      for (const chip of chips) {
        chip.classList.toggle('off', state.disabledOps.has(chip.dataset.name));
      }
    }
  };

  for (const { group, ops } of notation) {
    const box = document.createElement('div');
    box.className = 'notation-group';
    const head = document.createElement('label');
    head.className = 'check';
    head.innerHTML = `<input type="checkbox"> <span>${group}</span> <span class="count"></span>`;
    head.querySelector('input').addEventListener('change', (e) => {
      for (const [, name] of ops) {
        if (e.target.checked) state.disabledOps.delete(name);
        else state.disabledOps.add(name);
      }
      render();
      onChange();
    });
    box.append(head);

    const row = document.createElement('div');
    row.className = 'ops';
    const chips = ops.map(([mark, name]) => {
      const chip = document.createElement('button');
      chip.className = 'op';
      chip.dataset.name = name;
      chip.textContent = mark;
      chip.title = name;
      chip.addEventListener('click', () => {
        if (state.disabledOps.has(name)) state.disabledOps.delete(name);
        else state.disabledOps.add(name);
        render();
        onChange();
      });
      row.append(chip);
      return chip;
    });
    box.append(row);
    root.append(box);
    groups.push({ box, chips, ops });
  }
  render();
}

function escapeAttr(s) {
  return String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
}
