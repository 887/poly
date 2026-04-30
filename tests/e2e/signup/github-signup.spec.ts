/**
 * github-signup.spec.ts
 *
 * Asserts the GitHub "Register" link on /signup/github.
 *
 * github.com: https://github.com/signup
 * GitHub Enterprise (<server_url> set): href is the instance root (SSO landing).
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { test } from '@playwright/test';
import { makeExternalSignupSpec } from '../lib/signup-link-spec';

// Public GitHub.
makeExternalSignupSpec(
  'github',
  /^https:\/\/github\.com\/signup/,
);

// GitHub Enterprise — link should be the instance root (GHES uses SSO).
const GHE_HOST = 'https://github.mycorp.internal';
const ghePattern = new RegExp(
  `^${GHE_HOST.replace(/\./g, '\\.').replace(/\//g, '\\/')}`,
);

test.describe('github register link — GitHub Enterprise', () => {
  makeExternalSignupSpec('github', ghePattern, {
    fillServerUrl: GHE_HOST,
  });
});
