// SPDX-License-Identifier: MIT OR Apache-2.0
//
// The notation this host writes in: one mark per Bitcoin Script opcode.
//
// The engine has no opinion about any of it. Which characters are marks, what
// they are called and what a page does with them is the host's own affair —
// the engine is handed the boxes the host measured and the outlines it
// sampled, and grows vines off whichever of them the host says are marks.
// This file is that list, and nothing here crosses into the wasm module except
// as measured rectangles.
//
// 110 opcodes are defined as of this writing, and all 110 have a mark of their
// own. Marks are matched against the page's text longest-first, so `⊘¬⟨`
// (VERNOTIF) is never read as `⊘` (RESERVED) followed by a NOTIF.

export const NOTATION = [
  {
    group: 'Constants',
    ops: [
      ['⓪', '0'], ['⊖', '1NEGATE'], ['①', '1'], ['②', '2'], ['③', '3'],
      ['④', '4'], ['⑤', '5'], ['⑥', '6'], ['⑦', '7'], ['⑧', '8'],
      ['⑨', '9'], ['⑩', '10'], ['⑪', '11'], ['⑫', '12'], ['⑬', '13'],
      ['⑭', '14'], ['⑮', '15'], ['⑯', '16'],
    ],
  },
  {
    group: 'Flow control',
    ops: [
      ['°', 'NOP'], ['⟨', 'IF'], ['¬⟨', 'NOTIF'], ['│', 'ELSE'], ['⟩', 'ENDIF'],
      ['✓', 'VERIFY'], ['¶', 'RETURN'],
    ],
  },
  {
    group: 'Stack',
    ops: [
      ['⇥', 'TOALTSTACK'], ['⇤', 'FROMALTSTACK'], ['⌄₂', '2DROP'], ['⧉₂', '2DUP'],
      ['⧉₃', '3DUP'], ['⇗₂', '2OVER'], ['↻₂', '2ROT'], ['⇄₂', '2SWAP'],
      ['⧉?', 'IFDUP'], ['↕', 'DEPTH'], ['⌄', 'DROP'], ['⧉', 'DUP'], ['⌦', 'NIP'],
      ['⇗', 'OVER'], ['⇡', 'PICK'], ['⥀', 'ROLL'], ['↻', 'ROT'], ['⇄', 'SWAP'],
      ['⇘', 'TUCK'],
    ],
  },
  {
    group: 'Splice',
    ops: [
      ['⧺', 'CAT'], ['⊂', 'SUBSTR'], ['↤', 'LEFT'], ['↦', 'RIGHT'], ['ℓ', 'SIZE'],
    ],
  },
  {
    group: 'Bitwise and equality',
    ops: [
      ['∼', 'INVERT'], ['∩', 'AND'], ['∪', 'OR'], ['⊻', 'XOR'], ['=', 'EQUAL'],
      ['≡', 'EQUALVERIFY'],
    ],
  },
  {
    group: 'Arithmetic',
    ops: [
      ['+₁', '1ADD'], ['−₁', '1SUB'], ['×₂', '2MUL'], ['÷₂', '2DIV'], ['∓', 'NEGATE'],
      ['|·|', 'ABS'], ['¬', 'NOT'], ['≠₀', '0NOTEQUAL'], ['+', 'ADD'], ['−', 'SUB'],
      ['×', 'MUL'], ['÷', 'DIV'], ['%', 'MOD'], ['«', 'LSHIFT'], ['»', 'RSHIFT'],
      ['∧', 'BOOLAND'], ['∨', 'BOOLOR'],
    ],
  },
  {
    group: 'Comparison',
    ops: [
      ['≐', 'NUMEQUAL'], ['≑', 'NUMEQUALVERIFY'], ['≠', 'NUMNOTEQUAL'],
      ['<', 'LESSTHAN'], ['>', 'GREATERTHAN'], ['≤', 'LESSTHANOREQUAL'],
      ['≥', 'GREATERTHANOREQUAL'], ['⊓', 'MIN'], ['⊔', 'MAX'], ['∈', 'WITHIN'],
    ],
  },
  {
    group: 'Cryptography',
    ops: [
      ['ρ', 'RIPEMD160'], ['σ', 'SHA1'], ['Σ', 'SHA256'], ['⌖', 'HASH160'],
      ['⌘', 'HASH256'], ['‖', 'CODESEPARATOR'], ['∇', 'CHECKSIG'],
      ['▼', 'CHECKSIGVERIFY'], ['◇', 'CHECKMULTISIG'], ['◆', 'CHECKMULTISIGVERIFY'],
      ['∇₊', 'CHECKSIGADD'],
    ],
  },
  {
    group: 'Timelocks',
    ops: [['τ', 'CHECKLOCKTIMEVERIFY'], ['Δ', 'CHECKSEQUENCEVERIFY']],
  },
  {
    group: 'No-ops',
    ops: [
      ['°₁', 'NOP1'], ['°₄', 'NOP4'], ['°₅', 'NOP5'], ['°₆', 'NOP6'], ['°₇', 'NOP7'],
      ['°₈', 'NOP8'], ['°₉', 'NOP9'], ['°₁₀', 'NOP10'],
    ],
  },
  {
    group: 'Reserved and invalid',
    ops: [
      ['⊘', 'RESERVED'], ['⊘ᵛ', 'VER'], ['⊘⟨', 'VERIF'], ['⊘¬⟨', 'VERNOTIF'],
      ['⊘₁', 'RESERVED1'], ['⊘₂', 'RESERVED2'], ['☒', 'INVALIDOPCODE'],
    ],
  },
];

/** Every mark, longest first — the order a scanner has to try them in. */
export function marksByLength() {
  const marks = [];
  for (const { group, ops } of NOTATION) {
    for (const [mark, name] of ops) marks.push({ mark, name, group });
  }
  return marks.sort((a, b) => [...b.mark].length - [...a.mark].length);
}

/**
 * The opcode a term opens with, or null.
 *
 * A term is matched by PREFIX rather than whole: a script writes `⌖«hash»`
 * and `⧉₂` alike with no space, and the mark is what the term begins with.
 */
export function opcodeAt(text, marks) {
  for (const m of marks) {
    if (text.startsWith(m.mark)) return m;
  }
  return null;
}

/**
 * The default page: three scripts and the prose that introduces them.
 *
 * Data pushes are written `‹…›` rather than with the guillemets a script
 * listing usually reaches for, because `«` is LSHIFT and a page cannot use one
 * of its own marks as punctuation without the scanner reading it as notation —
 * which it would be right to do.
 */
export const DEFAULT_TEXT = `Every mark on this page is an opcode, and every opcode is a mark. A
scribe copying a script had no other way to say what a machine does than
to draw it, and the drawing is the notation.

⧉ ⌖ ‹pubKeyHash› ≡ ∇

⟨ ⧉ ⌖ ‹hot› ≡ ∇ │ ‹4032› Δ ⌄ ⧉ ⌖ ‹cold› ≡ ∇ ⟩

② ‹alice› ‹bob› ‹carol› ③ ◇ ✓

The vine grows off a mark and runs the silhouette of the block that mark
belongs to, counter-clockwise, from the point of that silhouette the mark
itself is touching.`;
