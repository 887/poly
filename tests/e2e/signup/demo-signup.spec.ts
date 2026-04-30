/**
 * demo-signup.spec.ts
 *
 * Asserts that the demo backend renders NO "Register" link.
 *
 * The demo backend returns SignupMethod::NotSupported, so RegisterLink
 * renders nothing. No data-testid="register-link-demo" should appear
 * anywhere on /signup/demo, and no register-link-* element should appear
 * inside signup-form-container.
 */

import { makeNotSupportedSignupSpec } from '../lib/signup-link-spec';

makeNotSupportedSignupSpec('demo');
