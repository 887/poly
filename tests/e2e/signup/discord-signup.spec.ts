/**
 * discord-signup.spec.ts
 *
 * Asserts the Discord "Register" link on /signup/discord has the correct
 * external URL: https://discord.com/register
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { makeExternalSignupSpec } from '../lib/signup-link-spec';

makeExternalSignupSpec(
  'discord',
  /^https:\/\/discord\.com\/register/,
);
