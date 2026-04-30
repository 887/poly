# Scenario: fact-handoff

Persona A (fact-alice) pins a fact via `meta_persona_set_memory(pinned=true)`. Persona
B (fact-bob) — a different slug with no source overlap — runs
`meta_persona_get_memory(slug=fact-alice)` and must receive the pinned fact.

**Regression this catches:** Cross-persona memory reads are deliberately allowed in the
v1 schema — there are no per-persona ACLs on the `persona_facts` table. If a future
change adds a persona-slug ACL gate that prevents fact-bob from reading fact-alice's
memory, this scenario fails loud. It serves as the explicit contract test documenting
the intentional shared-memory design at v1.
