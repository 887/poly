/**
 * poly-server-signup.spec.ts
 *
 * Asserts the poly-server "Register" link uses the InApp flow.
 *
 * Behaviour (Phase D):
 *   - On the /signup picker page: link is visible, clicking navigates to
 *     /signup/poly and shows signup-form-container.
 *   - On /signup/poly itself: Phase D hides the link (already on the route).
 *
 * No external URLs opened — the link is a Dioxus in-app router Link.
 */

import { makeInAppSignupSpec } from '../lib/signup-link-spec';

// The slug in register_link.rs for poly-server is "poly" (the SignupEntry slug),
// but the data-testid uses backend_slug which may be "poly-server". Verify
// against register_link.rs Phase D implementation:
//   - If backend_slug is "poly" → testid is "register-link-poly"
//   - If backend_slug is "poly-server" → testid is "register-link-poly-server"
//
// The plan table uses "poly-server" as the slug everywhere. The existing
// SignupEntry in clients/server-client/src/signup.rs uses slug "poly".
// Phase D should wire the RegisterLink with whichever slug the backend uses.
// This spec tests with "poly-server" per the Phase E task spec; if Phase D
// used "poly", update the backendId below and the scenario README accordingly.
makeInAppSignupSpec('poly-server', '/signup/poly');
