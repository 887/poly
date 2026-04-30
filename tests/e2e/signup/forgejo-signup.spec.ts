/**
 * forgejo-signup.spec.ts
 *
 * Asserts the Forgejo "Register" link on /signup/forgejo.
 *
 * Default instance: https://codeberg.org/user/sign_up
 * Custom instance: <server_url>/user/sign_up
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { test } from '@playwright/test';
import { makeExternalSignupSpec } from '../lib/signup-link-spec';

// Default Forgejo/Codeberg instance.
makeExternalSignupSpec(
  'forgejo',
  /^https:\/\/codeberg\.org\/user\/sign_up/,
);

// Custom Forgejo instance.
const CUSTOM_INSTANCE = 'https://git.mycorp.internal';
const customPattern = new RegExp(
  `^${CUSTOM_INSTANCE.replace(/\./g, '\\.')}\/user\/sign_up`,
);

test.describe('forgejo register link — custom instance', () => {
  makeExternalSignupSpec('forgejo', customPattern, {
    fillServerUrl: CUSTOM_INSTANCE,
  });
});
