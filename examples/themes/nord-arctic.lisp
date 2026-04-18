; ============================================================
;  nord-arctic — the canonical tatara look, as a Lisp form.
;
;  This file is parseable by `tatara_ui::ThemeSpec::compile_from_sexp`.
;  The BLAKE3 of the resulting spec IS the theme's stable identity
;  (`ThemeSpec::id()` → `theme:<16-char-short-hash>`).
;
;  Every tool in pleme-io loads ONE theme file — themes compose by
;  `:extends`, so org-wide customizations are a single override:
;
;    (deftheme my-brand
;      :extends "nord-arctic"
;      :semantic (:accent "#D08770"))    ; swap purple → orange
; ============================================================

(deftheme
  :name        "nord-arctic"
  :description "the canonical tatara look — Nord palette, Aurora semantic roles"
  :semantic    (:primary "#88C0D0"    ; frost cyan  — hero, banners, primary emphasis
                :accent  "#B48EAD"    ; aurora purple — accent text
                :info    "#81A1C1"    ; frost blue   — info / flow / becomes
                :success "#A3BE8C"    ; aurora green — realized, cached-hit OK
                :warn    "#EBCB8B"    ; aurora yellow — warnings, modifications
                :error   "#BF616A"    ; aurora red   — failures
                :dim     "#4C566A"))  ; polar night  — dim / muted / hashes
