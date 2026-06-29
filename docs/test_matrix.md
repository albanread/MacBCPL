# Test matrix

The **test matrix** is MacBCPL's in-tree behavioural conformance grid: a set of
small, self-contained probes organised by language layer, each embedding its own
BCPL source and an expected result. Unlike the *corpus* sweeps
(`lexer_corpus` / `parser_corpus` / `sema_corpus`, which walk an external
`reference/tests/bcl_tests/` directory and skip when it isn't vendored), the
matrix carries its own programs, so it always runs and always gates commits.

**Status: 302 / 302 green** (tiers 1–7 + extra + generated).

> This document was reconstructed from the live cells after the original was lost
> in a docs reorg. The tier files are authoritative; if you add or remove a cell,
> update the inventory here so the two stay in agreement.

## How it's organised

Seven tiers, one per language layer, plus two cross-cutting files:

| Tier | File | Cells | Layer |
|------|------|------:|-------|
| 1 | `tests/newbcpl-tests/tests/matrix_tier1_negatives.rs` | 14 | Lexical & syntactic — **rejection** |
| 2 | `tests/newbcpl-tests/tests/matrix_tier2.rs` | 25 | Sema positives (acceptance + correct downstream behaviour) |
| 3 | `tests/newbcpl-tests/tests/matrix_tier3.rs` | 55 | Expressions (every operator × applicable operand type) |
| 4 | `tests/newbcpl-tests/tests/matrix_tier4.rs` | 52 | Statements & control flow |
| 5 | `tests/newbcpl-tests/tests/matrix_tier5.rs` | 55 | Classes & methods (the object model) |
| 6 | `tests/newbcpl-tests/tests/matrix_tier6.rs` | 31 | Runtime & memory (heap / vectors / lists / builtins) |
| 7 | `tests/newbcpl-tests/tests/matrix_tier7.rs` | 20 | SIMD lane types (PAIR/FPAIR/QUAD/OCT/FQUAD) |
| — | `tests/newbcpl-tests/tests/matrix_extra.rs` | 33 | Cross-tier probes, macro-authored |
| — | `tests/newbcpl-tests/tests/matrix_generated.rs` | 17 | Generated from a manifest |
| | **Total** | **302** | |

## Authoring modes

- **Hand-written** (tiers 1–7): each cell is a `#[test] fn cellname()` that calls
  a runner helper with `(name, source, expected)`. The runner writes the source
  to a temp file, runs `newbcpl-driver run <file>` as a subprocess, and compares
  captured stdout (positives) or the non-zero exit + stderr substring
  (rejections).
- **Macro / manifest** (`matrix_extra`, `matrix_generated`): cells written
  declaratively. `matrix_generated.rs` is **generated — do not hand-edit it**;
  edit the manifest at `tests/newbcpl-tests/matrix/generated.matrix` and
  regenerate.

### Probe kinds

| Kind | Meaning |
|------|---------|
| `probe` | compile + run; stdout must **equal** the expected string |
| `probe_contains` | compile + run; stdout must **contain** the expected string |
| `reject` | must **fail to compile/run**: non-zero exit + stderr contains the diagnostic substring |

The matrix tests both *acceptance* (a valid program produces the right output)
and *rejection* (malformed input fails cleanly) — the two are different classes
of bug and are deliberately split (Tier 1 is the rejection dual of the positive
tiers).

## Running it

```sh
# the whole matrix
cargo test -p newbcpl-tests \
  --test matrix_tier1_negatives --test matrix_tier2 --test matrix_tier3 \
  --test matrix_tier4 --test matrix_tier5 --test matrix_tier6 --test matrix_tier7 \
  --test matrix_extra --test matrix_generated

# one tier
cargo test -p newbcpl-tests --test matrix_tier5
```

(Requires the LLVM env: `LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm`.)

## Regenerating the generated tier

```sh
cargo run -p newbcpl-test-matrix -- \
  tests/newbcpl-tests/matrix/generated.matrix \
  tests/newbcpl-tests/tests/matrix_generated.rs
```

Manifest row format:

```
<kind> <name> ::= <source> ==> <expected>
#   kind = probe | probe_contains | reject
#   escapes in source/expected: \n \t \r \\ \"
```

---

## Tier inventories

### Tier 1 — Lexical & syntactic (rejection) · 14 cells

Each cell is a malformed fragment paired with a diagnostic substring the
lexer/parser must emit. A sema rule isn't real until something enforces it;
these are the guard against a refactor silently starting to accept bad input.

`unterminated_string_rejected`, `empty_let_has_no_initialiser_rejected`,
`class_header_without_body_rejected`, `routine_without_body_rejected`,
`foreach_without_in_rejected`, `unbalanced_bcpl_brackets_rejected`,
`test_keyword_without_else_rejected`, `switchon_without_block_rejected`,
`let_count_mismatch_rejected`, `for_without_to_clause_rejected`,
`class_member_unknown_kind_rejected`, `virtual_without_method_keyword_rejected`,
`manifest_without_bindings_rejected`,
`comma_separated_targets_need_assign_rejected`

### Tier 2 — Sema positives · 25 cells

Each cell targets a sema rule by its observable consequence: a wrong type hint
routes to the wrong codegen op (int-add vs float-add, vec-len vs list-len), so a
regression surfaces as a stdout mismatch even though sema has no direct output.

`flet_with_float_literals`, `flet_coerces_int_literal_to_float`,
`flet_chain_propagates_float`, `manifest_substitutes_into_arithmetic`,
`manifest_drives_vec_allocation_size`, `multiple_manifests_in_one_block`,
`manifest_arithmetic_constants_fold_at_lower_time`,
`as_integer_annotation_compiles`, `as_pointer_annotation_compiles`,
`as_list_of_integer_annotation_compiles`, `valof_as_integer_annotation_accepted`,
`nested_let_inherits_outer_bindings`, `inner_let_shadows_outer_within_block`,
`for_loop_variable_is_block_scoped`, `function_locals_dont_leak_into_caller`,
`class_field_word_default_holds_int`, `new_propagates_class_hint_to_let_binding`,
`let_alias_propagates_class_hint`, `self_carries_class_in_method_body`,
`list_constructor_hint_picks_list_foreach`,
`vec_constructor_hint_picks_vec_foreach`,
`user_function_int_return_used_in_arithmetic`,
`user_function_float_return_used_in_arithmetic`,
`parallel_let_bindings_evaluate_left_to_right`,
`parallel_let_destructures_pair`

### Tier 3 — Expressions · 55 cells

Every operator on every applicable operand type. Three operator flavours coexist:
plain (`+`, `<`, …) for integer/pointer, dot-suffixed (`+.`, `<.`) for explicit
float, and hash-suffixed (`+#`, `<#`) as a float alias the corpus uses heavily —
where a row applies to both float syntaxes there's a probe per form.

`int_add`, `int_sub`, `int_mul`, `int_div_floor`, `int_rem`, `int_unary_neg`,
`int_arith_precedence_mul_before_add`, `int_arith_parens_override_precedence`,
`float_add_dot`, `float_mul_dot`, `float_div_dot`, `float_add_hash`,
`float_mul_hash`, `float_builtin_promotes_int`, `int_eq_true_path`,
`int_eq_false_path`, `int_ne_compares`, `int_lt_strict`, `int_le_inclusive`,
`int_gt_ge_pair`, `bit_and`, `bit_or`, `bit_shl`, `bit_shr_arithmetic`,
`word_form_band_bor`, `word_form_logical_and_or`, `conditional_expr_true_branch`,
`conditional_expr_false_branch`, `conditional_expr_nested`, `vec_word_subscript`,
`fvec_float_subscript`, `let_bindings_compose`, `flet_binding_inferred_float`,
`user_function_returning_int`, `recursive_function_terminates`, `nested_call`,
`bitfield_read_low_byte`, `bitfield_read_high_byte`,
`bitfield_read_single_bit_default_width`, `bitfield_write_inserts_field`,
`bitfield_write_preserves_other_bits`, `eqv_equal_operands_is_true`,
`eqv_unequal_operands_is_false`, `neqv_xor_returns_bitwise_difference`,
`neqv_equal_operands_zero`, `address_of_round_trips_through_indirection`,
`char_lit_plain_ascii`, `char_lit_escape_newline`, `char_lit_escape_tab`,
`char_lit_escape_space`, `char_lit_escape_backspace`, `char_lit_escape_newpage`,
`char_lit_escape_carriage_return`, `char_lit_escape_double_quote`,
`char_lit_escape_asterisk`

### Tier 4 — Statements & control flow · 52 cells

Every control-flow construct in the dialect; each asserts on a small output that
identifies which path the runtime actually took. Includes globals, `GET`
includes, mutual recursion, the classical `LET … AND` chain, and the `BRK`
debugger surfaces.

`if_then_taken`, `if_then_skipped`, `unless_inverts_condition`,
`test_then_else_true`, `test_then_else_false`, `if_else_chain`,
`while_iterates_until_false`, `while_zero_iterations`, `until_iterates_while_false`,
`repeat_while_runs_at_least_once`, `repeat_until_runs_at_least_once`,
`for_default_step_is_one`, `for_explicit_step_by_two`,
`for_zero_iterations_if_start_above_end`, `break_exits_innermost_loop`,
`loop_skips_to_next_iteration`, `break_only_exits_inner_when_nested`,
`valof_returns_resultis_value`, `valof_short_circuits_after_resultis`,
`switchon_matches_case`, `switchon_falls_through_to_default`,
`switchon_endcase_jumps_to_end`, `forward_goto_skips_block`,
`nested_blocks_inherit_outer_scope`, `nested_blocks_shadow_outer_name`,
`while_inside_if`, `if_inside_for`, `goto_forward_jumps_over_code`,
`goto_into_loop_body`, `global_single_form_writes_visible_in_start`,
`global_block_form_writes_visible_in_start`, `global_seen_from_separate_routine`,
`globals_slot_form_rejected`, `global_colon_slot_syntax_rejected`,
`get_pulls_manifest_from_sibling_file`, `get_pulls_manifest_from_modules_active`,
`get_missing_file_rejected`,
`mutual_recursion_via_consecutive_lets_terminates`,
`mutual_recursion_routines_with_be_bodies`,
`classical_let_and_chain_two_functions`, `classical_let_and_chain_three_routines`,
`classical_let_and_chain_mixes_function_and_routine`,
`expression_and_still_works_when_not_followed_by_paren`,
`brk_emits_banner_with_routine_name_and_line`, `brk_emits_heap_summary`,
`brk_emits_register_state`, `brk_emits_stack_walk`,
`brk_reports_routine_name_from_helper`, `brk_does_not_halt_program`,
`brk_stack_frame_resolves_routine_name`,
`brk_two_deep_call_chain_names_each_frame`, `get_cycle_rejected`

### Tier 5 — Classes & methods · 55 cells

The object model — the tier where the class-shape bugs lived (LET-vs-DECL fields,
ROUTINE/`=`-expr method forms, default-RELEASE-slot null calls, `BE { … }`
bodies). Covers class shapes, field/method forms, `USING` (RAII release),
field initialisers, `SUPER`, virtual dispatch, `FINAL`, param-`AS`-class
dispatch, indirect dispatch, and public/private/protected visibility.

`class_shape_bcpl_brackets`, `class_shape_c_braces`, `class_shape_be_marker`,
`field_decl_classic`, `field_decl_let_no_init`, `method_routine_be_stmt`,
`method_function_eq_expr`, `method_routine_eq_expr_swap`,
`method_let_routine_form`, `default_release_does_not_segfault`,
`no_explicit_create_still_constructs`, `bare_field_name_inside_method`,
`method_calls_sibling_method`, `multiple_instances_isolate_state`,
`chain_field_then_method`, `chain_method_then_method`,
`chain_via_decl_as_class_annotation`, `chain_via_as_class_annotation`,
`using_fall_through_runs_release`, `using_release_runs_before_early_return`,
`using_release_runs_before_finish`, `nested_using_releases_innermost_first`,
`using_binding_supports_method_calls_in_body`, `break_out_of_using_runs_release`,
`loop_through_using_runs_release_each_iteration`,
`endcase_through_using_runs_release`, `break_releases_inner_using_only`,
`field_initialiser_runs_at_new_no_user_create`,
`field_initialisers_prepended_to_user_create`,
`class_typed_field_initialiser_resolves_chain`, `setter_then_getter`,
`super_create_runs_parent_init`, `super_method_call_reaches_parent_body`,
`virtual_method_dispatches_to_override`,
`virtual_dispatch_picks_subclass_body_via_vtable`,
`final_method_callable_when_not_overridden`, `final_method_override_rejected`,
`final_override_rejected_through_chain`, `non_final_override_still_allowed`,
`function_param_as_class_dispatches_method`,
`routine_param_as_class_accesses_field`, `class_method_param_as_class_chains`,
`param_annotation_enforces_visibility`,
`param_without_annotation_workaround_via_typed_local`,
`indirect_dispatch_resolves_method_on_untyped_param`,
`indirect_dispatch_routes_to_dynamic_class`, `indirect_dispatch_passes_arguments`,
`indirect_dispatch_works_in_routine_body`,
`public_field_accessible_from_outside`, `private_field_rejected_from_outside`,
`private_field_accessible_from_inside`, `protected_field_rejected_from_outside`,
`protected_field_accessible_in_subclass`, `private_method_rejected_from_outside`,
`private_method_callable_through_public_wrapper`

### Tier 6 — Runtime & memory · 31 cells

Drives each landed runtime surface end-to-end through the JIT (lower-level GC
invariants are unit-tested in `newbcpl-runtime`; this tier covers what a BCPL
program touches): `NEW`, `VEC`/`FVEC`/the `*GETVEC` family, the cons-cell list
ops (`HD`/`TL`/`LEN`/`APND`/`CONCAT`/`FOREACH`), `GC()`/`HEAP_INFO()`,
trace-through-fields under alloc pressure, panic unwinding, and `FINISH`.

`new_class_round_trips_a_value`, `vec_holds_its_length_at_negative_one`,
`vec_subscript_reads_what_was_written`, `vec_init_list_reads_each_slot`,
`fvec_holds_floats`, `igetvec_allocates_integer_vector`,
`sgetvec_allocates_string_vector`, `pgetvec_allocates_pair_vector`,
`qgetvec_allocates_quad_vector`, `list_len_counts_appends`,
`list_hd_returns_first_element`, `list_tl_skips_one_element`,
`apnd_grows_an_empty_list`, `foreach_walks_list_chain_in_order`,
`foreach_walks_vec_by_index`, `gc_returns_zero_and_keeps_going`,
`heap_info_prints_its_header`, `heap_info_after_alloc_shows_block`,
`many_allocations_do_not_overlap`, `vec_of_class_pointers_round_trip`,
`declared_field_back_filled_is_traced`, `as_annotated_let_field_is_traced`,
`traced_field_survives_alloc_pressure`, `deep_chain_survives_collection`,
`runtime_panic_unwinds_through_jit`, `runtime_panic_unwinds_through_nested_call`,
`finish_terminates_cleanly`, `list_concat_combines_two_chains`,
`list_concat_walked_through_hd_tl_chain`,
`retain_declares_binding_and_survives_gc`,
`utf8_multibyte_glyph_reads_as_one_code_point`

### Tier 7 — SIMD lane types · 20 cells

Locks in the normative lane widths from `docs/pair_and_multilane_types.md`:
PAIR/FPAIR/QUAD/OCT pack into one i64 word with sign-aware shift-extract; FQUAD is
`<4 x f32>` via `extractelement`; FOREACH-destructuring unpacks lanes per
iteration. Covers construction, lane read (constant + runtime index), sign
extension, lane write, fields holding lane types, and list destructuring.

`pair_construct_and_extract_lane_zero`, `pair_extract_lane_one`,
`pair_negative_lane_sign_extends`, `pair_zero_lanes_round_trip`,
`pair_extreme_values_at_lane_boundary`, `pair_let_destructure`,
`quad_construct_and_extract_each_lane`, `quad_negative_lane_sign_extends`,
`oct_construct_and_extract_lanes`, `oct_negative_lane_sign_extends`,
`foreach_pair_destructure_walks_chain`, `foreach_pair_destructure_empty_list`,
`class_field_holding_pair_round_trips`, `pair_runtime_lane_index_zero`,
`pair_runtime_lane_index_one`, `pair_lane_write_constant_index`,
`pair_lane_write_high_lane`, `pair_lane_write_runtime_index`,
`pair_lane_write_into_field`, `quad_lane_write_preserves_other_lanes`

### Extra · 33 cells (`matrix_extra.rs`)

Cross-tier probes written with the `probe!` / `probe_contains!` / `reject!`
macros (named cells include `lowercase_writes_resolves`,
`lowercase_writen_resolves`, `lowercase_writef_arity_dispatch`, …). Adding a row
is one line; over time these migrate into the per-tier files or the manifest.

### Generated · 17 cells (`matrix_generated.rs`)

Emitted from `tests/newbcpl-tests/matrix/generated.matrix` by the
`newbcpl-test-matrix` crate. Currently Tier-3 arithmetic edge cases and Tier-4
control-flow combinations (e.g. `int_max_arith`, `int_neg_div`,
`int_double_negation`, negative-step FOR loops). The manifest is the source of
truth — see the regeneration command above.

---

## Relationship to the rest of the suite

The 302 matrix cells are a subset of the full `cargo test --workspace` run. The
remainder are crate-level unit tests (`newbcpl-lexer` / `-parser` / `-sema` /
`-ir` / `-llvm` / `-runtime`) and the other integration files
(`lists`, `strings`, `objc`, `arena`, `asm_probes`, the `*_smoke` files). The
optional corpus sweeps (`lexer_corpus` / `parser_corpus` / `sema_corpus`) are
gated behind `NEWBCPL_*_CORPUS=1` and require a vendored
`reference/tests/bcl_tests/`; absent that directory they skip.
