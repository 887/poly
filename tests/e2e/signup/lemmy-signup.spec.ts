/**
 * lemmy-signup.spec.ts
 *
 * Asserts the Lemmy "Register" link on /signup/lemmy.
 *
 * Default instance: https://lemmy.ml/signup
 * Custom instance: <server_url>/signup
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { test } from '@playwright/test';
import { makeExternalSignupSpec } from '../lib/signup-link-spec';

// Default Lemmy instance.
makeExternalSignupSpec(
  'lemmy',
  /^https:\/\/lemmy\.ml\/signup/,
);

// Custom Lemmy instance.
const CUSTOM_INSTANCE = 'https://beehaw.org';
const customPattern = new RegExp(
  `^${CUSTOM_INSTANCE.replace(/\./g, '\\.')}\/signup`,
);

test.describe('lemmy register link — custom instance', () => {
  makeExternalSignupSpec('lemmy', customPattern, {
    fillServerUrl: CUSTOM_INSTANCE,
  });
});
