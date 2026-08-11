/* tslint:disable */
/* eslint-disable */

/**
 * One block's convex hull, at whatever margin the caller asks for.
 *
 * `illuminate` already reports the ring growth actually rode, which is the
 * hull grown clear of the text's own halo. This is for a host that wants the
 * UNGROWN hull as well — at `pad` 0 its vertices are points on the letters
 * themselves, which is the only way to see that the silhouette really is the
 * writing's own shape and not a box around it.
 */
export function blockHull(glyphs: any, pad: number): any;

/**
 * The default stage ladder.
 */
export function defaultGrowthStages(): any;

/**
 * The default tunables. A host's control panel binds to these and hands the
 * mutated object back with every call, so there is exactly one definition of
 * what a knob starts at.
 */
export function defaultParams(): any;

/**
 * The symbol string for one source at one stage — the grammar without the
 * geometry.
 */
export function generateSymbol(seed: string, stage: number, boost: number, params: any, stages: any): string;

/**
 * Which stage a depth count falls in. Pass `null` for the default ladder.
 */
export function growthStage(confirmations: number, stages: any): number;

/**
 * Whether a character has an outline to ride — for a host that wants to know
 * before it goes measuring ink.
 */
export function hasOutline(ch: string): boolean;

/**
 * Grows one layout's worth of decoration.
 */
export function illuminate(request: any): any;

/**
 * Routes a Rust panic to the browser console instead of an opaque
 * `unreachable`. Safe to call more than once.
 */
export function initPanicHook(): void;

/**
 * The leading whitespace of a text node, in UTF-16 code units.
 */
export function leadingSpaceLength(text: string): number;

/**
 * How much of a seed's text is the mark itself, in UTF-16 code units — the
 * host uses this directly as a DOM text offset. Pass `null` for no override.
 */
export function markLeadLength(text: string, override_len?: number | null): number;

/**
 * Drops every contour some larger contour contains — exposed so a host can
 * check the silhouette rule directly.
 */
export function outerContours(contours: any): any;

/**
 * Clears every cached derivation. Required after any GRAMMAR knob changes, or
 * a seed keeps answering with generations grown under the old rules.
 */
export function resetDerivations(): void;

/**
 * Registers the host's glyph outlines: `{ "⌘": "M…", … }`, each an SVG path in
 * a unit square. Replaces the whole table, dropping any cached sampling.
 */
export function setOutlines(outlines: any): void;

/**
 * The extra generations a mark's own rendered size earns it.
 */
export function sizeBoost(size: number, base_size: number, params: any): number;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly blockHull: (a: any, b: number) => [number, number, number];
    readonly defaultGrowthStages: () => [number, number, number];
    readonly defaultParams: () => [number, number, number];
    readonly generateSymbol: (a: number, b: number, c: number, d: number, e: any, f: any) => [number, number, number, number];
    readonly growthStage: (a: number, b: any) => [number, number, number];
    readonly hasOutline: (a: number, b: number) => number;
    readonly illuminate: (a: any) => [number, number, number];
    readonly initPanicHook: () => void;
    readonly leadingSpaceLength: (a: number, b: number) => number;
    readonly markLeadLength: (a: number, b: number, c: number, d: number) => number;
    readonly outerContours: (a: any) => [number, number, number];
    readonly resetDerivations: () => void;
    readonly setOutlines: (a: any) => [number, number];
    readonly sizeBoost: (a: number, b: number, c: any) => [number, number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
