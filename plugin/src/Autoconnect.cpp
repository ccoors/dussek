#include "Autoconnect.hpp"

#include <algorithm>
#include <cstring>

namespace pe {

std::vector<std::pair<std::string, std::string>> autoconnectPlan(const std::vector<std::string>& outs,
    const std::vector<std::string>& physical)
{
    std::vector<std::pair<std::string, std::string>> plan;
    if (physical.empty())
        return plan;
    // the first device: the ports whose client part (before the last ':') is the first port's
    const auto client = [](const std::string& p) { return p.substr(0, p.rfind(':')); };
    std::vector<std::string> device;
    for (const std::string& p : physical)
        if (client(p) == client(physical.front()))
            device.push_back(p);
    for (size_t i = 0; i < outs.size(); ++i)
        plan.emplace_back(outs[i], device[std::min(i, device.size() - 1)]);
    return plan;
}

bool autoconnectWanted(const char* env)
{
    return env == nullptr || std::strcmp(env, "0") != 0;
}

}
