/**
 * stoat-signup.spec.ts
 *
 * Asserts the Stoat "Register" link on /signup/stoat.
 *
 * Official instance: https://app.stoat.chat
 * Self-hosted: if server_url provided and differs from api.stoat.chat,
 * the href becomes the custom instance root URL.
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { test } from '@playwright/test';
import { makeExternalSignupSpec } from '../lib/signup-link-spec';

// Default: official Stoat instance.
makeExternalSignupSpec(
  'stoat',
  /^https:\/\/app\.stoat\.chat/,
);

// Self-hosted: custom server URL → href is the instance root.
const SELF_HOSTED = 'https://stoat.mycorp.internal';
const selfHostedPattern = new RegExp(
  `^${SELF_HOSTED.replace(/\./g, '\\.').replace(/\//g, '\\/')}`,
);

test.describe('stoat register link — self-hosted server URL', () => {
  makeExternalSignupSpec('stoat', selfHostedPattern, {
    fillServerUrl: SELF_HOSTED,
  });
});
