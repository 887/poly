/**
 * hackernews-signup.spec.ts
 *
 * Asserts the Hacker News "Register" link on /signup/hackernews.
 *
 * HN's login page is also the create-account page (it carries both "Login"
 * and "Create Account" buttons). There is no separate /signup URL.
 *
 * Expected href: https://news.ycombinator.com/login
 *
 * Mock-mode (CI): asserts href only.
 * Real-network (POLY_SIGNUP_E2E_REAL=1): clicks and verifies the page is reachable.
 */

import { makeExternalSignupSpec } from '../lib/signup-link-spec';

makeExternalSignupSpec(
  'hackernews',
  /^https:\/\/news\.ycombinator\.com\/login/,
);
