/* SPDX-License-Identifier: GPL-2.0 */
/* kunit/visibility.h shim: !CONFIG_KUNIT build — VISIBLE_IF_KUNIT
 * makes the symbol static, EXPORT_SYMBOL_IF_KUNIT is nothing (exactly
 * the upstream fallback branch). */
#ifndef _KUNIT_VISIBILITY_H
#define _KUNIT_VISIBILITY_H

#define VISIBLE_IF_KUNIT static
#define EXPORT_SYMBOL_IF_KUNIT(symbol)
#define EXPORT_SYMBOL_STATIC_IF_KUNIT(symbol)

#endif /* _KUNIT_VISIBILITY_H */
