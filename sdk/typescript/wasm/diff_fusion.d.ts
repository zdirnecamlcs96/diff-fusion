/* tslint:disable */
/* eslint-disable */
export function transform_to_cif(source: string, schema: string, format_id: string): string;
export function merge_field(change: string, policy_ref: string, ctx: string): string;
export function three_way_diff(ancestor: string, a: string, b: string): string;
export function compare_json(a: string, b: string): string;
export function canonical_json(doc: string): string;
export function fuse(ancestor: string, a: string, b: string, policy_doc: string, ctx: string): string;
export function idempotency_key_hex(canonical_id: string, operation: string, payload: string): string;
export function merge_batch(changelog: string, policy_doc: string, ctx: string): string;
