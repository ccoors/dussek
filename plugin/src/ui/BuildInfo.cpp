// The version (plugin/CMakeLists.txt's project) and the commit (generated at build time) for the
// About tab and the hosts; kept apart so a new commit compiles only this file again.
#include "About.hpp"
#include "BuildCommit.h"

namespace pe {
Version pluginVersion() { return { PE_VERSION_MAJOR, PE_VERSION_MINOR, PE_VERSION_PATCH }; }
std::string versionText()
{
    const Version v = pluginVersion();
    return std::to_string(v.major) + "." + std::to_string(v.minor) + "." + std::to_string(v.patch);
}
const char* buildCommit() { return PE_BUILD_COMMIT; }
} // namespace pe
