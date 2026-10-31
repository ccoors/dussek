#include "StateVersion.hpp"

#include <cstring>

namespace pe {

uint32_t parseStateVersion(const char* value)
{
    if (value == nullptr || *value == '\0')
        return kFirstStateVersion;
    uint64_t v = 0;
    for (const char* p = value; *p != '\0'; ++p) {
        if (*p < '0' || *p > '9' || v > 1000000)
            return kFirstStateVersion;
        v = v * 10 + (uint64_t)(*p - '0');
    }
    return v == 0 ? kFirstStateVersion : (uint32_t)v;
}

std::string migrateState(uint32_t from, const std::string& key, std::string value)
{
    // version 1 is the first released form: nothing to migrate yet. A migration from N to N + 1
    // goes here as `if (from <= N && key == …) value = …;`, in order.
    (void)from;
    (void)key;
    return value;
}

bool stateKnown(const std::string& key)
{
    for (const char* k : stateKeys())
        if (key == k)
            return true;
    return false;
}

std::string StateRestore::apply(const std::string& key, const std::string& value)
{
    if (key == kStateVersionKey) {
        fVersion = parseStateVersion(value.c_str());
        return value;
    }
    return migrateState(fVersion, key, value);
}

} // namespace pe
