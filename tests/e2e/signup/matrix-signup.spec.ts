/**
 * matrix-signup.spec.ts
 *
 * Asserts the Matrix "Register" link on /signup/matrix.
 *
 * Default (no server URL typed): href matches https://app.element.io/#/register
 * Custom server URL injected: href reflects <server>/_matrix/client/v3/register
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { test } from '@playwright/test';
import { makeExternalSignupSpec } from '../lib/signup-link-spec';

// Default homeserver — no server URL filled in.
makeExternalSignupSpec(
  'matrix',
  /^https:\/\/app\.element\.io\/#\/register/,
);

// Custom server URL — test suite variant that injects a known homeserver.
// The backend parameterises the URL as: <server>/_matrix/client/v3/register
const CUSTOM_SERVER = 'https://matrix.example.org';
const customPattern = new RegExp(
  `^${CUSTOM_SERVER.replace(/\./g, '\\.')}/_matrix/client/v3/register`,
);

// We declare this as a named group so it doesn't shadow the default group.
test.describe('matrix register link — custom server URL', () => {
  // Re-use the factory with fillServerUrl so the component parameterises the href.
  makeExternalSignupSpec('matrix', customPattern, {
    fillServerUrl: CUSTOM_SERVER,
  });
});
