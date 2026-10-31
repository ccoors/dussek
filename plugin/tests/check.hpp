// Minimal test harness for the plugin's pure logic (no window, no host).
#pragma once
#include <cmath>
#include <cstdio>
#include <functional>
#include <vector>

namespace check {
struct Case { const char* name; std::function<void()> fn; };
inline std::vector<Case>& cases() { static std::vector<Case> c; return c; }
inline int& failures() { static int f = 0; return f; }
struct Reg { Reg(const char* n, std::function<void()> f) { cases().push_back({ n, std::move(f) }); } };
inline int runAll()
{
    for (const Case& c : cases()) {
        const int before = failures();
        c.fn();
        std::printf("%s %s\n", failures() == before ? "ok  " : "FAIL", c.name);
    }
    std::printf("%zu tests, %d failed checks\n", cases().size(), failures());
    return failures() == 0 ? 0 : 1;
}
} // namespace check

#define TEST(name) static void name(); static check::Reg reg_##name(#name, name); static void name()
#define CHECK(c) do { if (!(c)) { ++check::failures(); std::printf("  %s:%d: CHECK(%s)\n", __FILE__, __LINE__, #c); } } while (0)
#define CHECK_NEAR(a, b, tol) do { const double a_ = (a), b_ = (b); if (!(std::fabs(a_ - b_) <= (tol))) { ++check::failures(); \
    std::printf("  %s:%d: %s = %g, %s = %g\n", __FILE__, __LINE__, #a, a_, #b, b_); } } while (0)
