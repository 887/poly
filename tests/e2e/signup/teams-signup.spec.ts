/**
 * teams-signup.spec.ts
 *
 * Asserts the Microsoft Teams "Register" link on /signup/teams has the
 * correct external URL: https://signup.live.com/signup?lic=1
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 *
 * Note: URL verified 2026-04-30. If this test starts failing with a 404,
 * re-verify the current MSA signup URL and update both this spec and
 * clients/teams/src/signup.rs.
 */

import { makeExternalSignupSpec } from '../lib/signup-link-spec';

makeExternalSignupSpec(
  'teams',
  /^https:\/\/signup\.live\.com\/signup/,
);
