# Plugin wrapper: DPF (C++) around the Rust engine (engine/, static library, C ABI).
#   just plugin   (fetches DPF at its pinned commit, configures plugin/build, builds)
cmake_minimum_required(VERSION 3.22)
# macOS: the oldest release the builds run on (libc++'s std::to_chars for floats: 13.3)
set(CMAKE_OSX_DEPLOYMENT_TARGET "13.3" CACHE STRING "the oldest macOS the builds run on")
# macOS: one build for both chips, Apple silicon and Intel (the engine built per chip and joined by lipo);
# -DCMAKE_OSX_ARCHITECTURES=arm64 builds one chip's only. Ignored elsewhere.
set(CMAKE_OSX_ARCHITECTURES "arm64;x86_64" CACHE STRING "the chips the macOS builds run on")
# the plugin's version: the hosts' (getVersion) and the About tab's
project(dussek-plugin VERSION 0.5.0 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 17)
# MinGW: its C and C++ runtimes linked in (libstdc++, libgcc, winpthread), so the builds run on a
# Windows without MSYS2; the system's DLLs stay shared (CI checks the imports: platforms.yml)
if(MINGW)
  add_link_options(-static)
endif()
set(CMAKE_CXX_STANDARD_REQUIRED ON)

if(NOT EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/dpf/CMakeLists.txt)
  message(FATAL_ERROR "DPF missing: run `just dpf`")
endif()
add_subdirectory(dpf)

# the engine, built by cargo
find_program(CARGO cargo REQUIRED)
set(ENGINE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../engine)
if(APPLE AND CMAKE_OSX_ARCHITECTURES)
  # macOS: the engine once per chip (rustup target add aarch64-apple-darwin x86_64-apple-darwin), then one
  # library with both (lipo)
  set(_engine_builds "")
  set(_engine_libs "")
  foreach(arch ${CMAKE_OSX_ARCHITECTURES})
    if(arch STREQUAL "arm64")
      set(_triple aarch64-apple-darwin)
    elseif(arch STREQUAL "x86_64")
      set(_triple x86_64-apple-darwin)
    else()
      message(FATAL_ERROR "CMAKE_OSX_ARCHITECTURES: no Rust target for ${arch}")
    endif()
    list(APPEND _engine_builds
      COMMAND ${CMAKE_COMMAND} -E env MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}
              ${CARGO} build --release --lib --target ${_triple} --manifest-path ${ENGINE_DIR}/Cargo.toml)
    list(APPEND _engine_libs ${ENGINE_DIR}/target/${_triple}/release/libdussek.a)
  endforeach()
  set(ENGINE_LIB ${CMAKE_CURRENT_BINARY_DIR}/engine/libdussek.a)
  add_custom_target(engine_cargo
    ${_engine_builds}
    COMMAND ${CMAKE_COMMAND} -E make_directory ${CMAKE_CURRENT_BINARY_DIR}/engine
    COMMAND lipo -create ${_engine_libs} -output ${ENGINE_LIB}
    BYPRODUCTS ${ENGINE_LIB}
    USES_TERMINAL
    COMMENT "cargo build (engine: ${CMAKE_OSX_ARCHITECTURES})")
else()
  set(ENGINE_LIB ${ENGINE_DIR}/target/release/${CMAKE_STATIC_LIBRARY_PREFIX}dussek${CMAKE_STATIC_LIBRARY_SUFFIX})
  add_custom_target(engine_cargo
    COMMAND ${CMAKE_COMMAND} -E env MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}
            ${CARGO} build --release --lib --manifest-path ${ENGINE_DIR}/Cargo.toml
    BYPRODUCTS ${ENGINE_LIB}
    USES_TERMINAL
    COMMENT "cargo build (engine)")
endif()
add_library(engine STATIC IMPORTED)
set_target_properties(engine PROPERTIES IMPORTED_LOCATION ${ENGINE_LIB})
if(WIN32)
  # what Rust's standard library links on Windows (cargo rustc -- --print native-static-libs)
  set_property(TARGET engine PROPERTY INTERFACE_LINK_LIBRARIES
    kernel32 advapi32 ntdll userenv ws2_32 bcrypt dbghelp synchronization)
endif()
add_dependencies(engine engine_cargo)
find_package(Threads REQUIRED)
find_library(RT_LIB rt)

# the two plugins, from the same sources: Dussek (the keyboard instruments) and Weidinger (the brass);
# each its own DistrhoPluginInfo.h (src/plugins/<name>/: the name, the IDs, PE_PLUGIN_KIND)
set(PE_PLUGINS dussek weidinger)
# macOS adds an AudioUnit (AUv2: Logic Pro and GarageBand load nothing else)
set(PE_TARGETS lv2 vst3 clap jack)
if(APPLE)
  list(APPEND PE_TARGETS au)
endif()
foreach(p ${PE_PLUGINS})
  dpf_add_plugin(${p}
    TARGETS ${PE_TARGETS}
    UI_TYPE opengl
    USE_FILE_BROWSER
    FILES_DSP src/PianoPlugin.cpp
    FILES_UI src/ui/PianoUI.cpp src/ui/Widgets.cpp src/ui/Panel.cpp src/ui/DiagView.cpp src/ui/DiagPanel.cpp src/ui/VizPanel.cpp src/ui/MicView.cpp)
  target_include_directories(${p} PUBLIC src src/plugins/${p})
  target_link_libraries(${p} PUBLIC Threads::Threads ${CMAKE_DL_LIBS} m)
  # the standalone's `render` command (midi2wav's code through pe_cli), before DPF's main, and
  # StandaloneMain.cpp so Windows standalone builds include WASAPI, ASIO, and DirectSound with
  # runtime audio engine and device selection.
  get_target_property(_jack_sources ${p}-jack SOURCES)
  list(FILTER _jack_sources EXCLUDE REGEX "DistrhoPluginMain\\.cpp$")
  list(APPEND _jack_sources
    "${CMAKE_CURRENT_SOURCE_DIR}/src/StandaloneMain.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/src/StandaloneCli.cpp")
  set_target_properties(${p}-jack PROPERTIES SOURCES "${_jack_sources}")
  target_include_directories(${p}-jack PRIVATE
    src/asio
    ${CMAKE_CURRENT_SOURCE_DIR}/dpf/distrho/src/jackbridge)
  if(WIN32)
    target_link_libraries(${p}-jack PRIVATE dsound ole32 advapi32 uuid)
  endif()
endforeach()

# each plugin its own DGL, under its own namespace: DPF builds one for every plugin of a build, and its
# Objective-C classes (pugl's window and views, named after DGL_NAMESPACE) are global to a process, so a
# macOS host with Dussek and Weidinger loaded had two of each and used one plugin's for both. The copy takes
# dgl-opengl's sources and settings; Weidinger's UI links it in dgl-opengl's place.
get_target_property(_dgl_sources dgl-opengl SOURCES)
add_library(dgl-opengl-weidinger STATIC ${_dgl_sources})
foreach(prop COMPILE_DEFINITIONS COMPILE_OPTIONS COMPILE_FEATURES INCLUDE_DIRECTORIES LINK_LIBRARIES
             INTERFACE_COMPILE_DEFINITIONS INTERFACE_COMPILE_OPTIONS INTERFACE_INCLUDE_DIRECTORIES
             INTERFACE_LINK_LIBRARIES INTERFACE_LINK_OPTIONS POSITION_INDEPENDENT_CODE
             C_VISIBILITY_PRESET CXX_VISIBILITY_PRESET VISIBILITY_INLINES_HIDDEN)
  get_target_property(_v dgl-opengl ${prop})
  if(NOT _v STREQUAL "_v-NOTFOUND")
    set_property(TARGET dgl-opengl-weidinger PROPERTY ${prop} "${_v}")
  endif()
endforeach()
target_compile_definitions(dgl-opengl PUBLIC DGL_NAMESPACE=DussekDGL)
target_compile_definitions(dgl-opengl-weidinger PUBLIC DGL_NAMESPACE=WeidingerDGL)
foreach(prop LINK_LIBRARIES INTERFACE_LINK_LIBRARIES)
  get_target_property(_v weidinger-ui ${prop})
  list(TRANSFORM _v REPLACE "^dgl-opengl$" "dgl-opengl-weidinger")
  list(TRANSFORM _v REPLACE "^\\$<LINK_ONLY:dgl-opengl>$" "$<LINK_ONLY:dgl-opengl-weidinger>")
  set_property(TARGET weidinger-ui PROPERTY ${prop} "${_v}")
endforeach()

# the standalones' window icon, each plugin's (in a host the window is the host's): on X11 DPF sets _NET_WM_ICON
# from the array PianoUI.cpp defines (src/ui/WindowIcon-<name>.inc, from packaging/icon/render.sh; the size its
# length, the same for both), on Windows from the icon resource compiled into each standalone; macOS's app carries
# its own (packaging/macos)
if(UNIX AND NOT APPLE)
  foreach(t dgl-opengl dgl-opengl-weidinger)
    target_compile_definitions(${t} PUBLIC DGL_X11_WINDOW_ICON_NAME=pe_window_icon DGL_X11_WINDOW_ICON_SIZE=21510)
  endforeach()
elseif(WIN32)
  enable_language(RC)
  foreach(t dgl-opengl dgl-opengl-weidinger)
    target_compile_definitions(${t} PUBLIC DGL_WINDOWS_ICON_ID=1)
  endforeach()
  foreach(p ${PE_PLUGINS})
    file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/generated/WindowIcon-${p}.rc
         "1 ICON \"${CMAKE_CURRENT_SOURCE_DIR}/../packaging/icon/${p}.ico\"\n")
    target_sources(${p}-jack PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/generated/WindowIcon-${p}.rc)
  endforeach()
endif()

# pure logic shared by the DSP and the UI, and its tests (no window)
add_library(pe_common STATIC src/Autoconnect.cpp src/Catalog.cpp src/Params.cpp src/Ports.cpp src/EngineValues.cpp src/PedalMixer.cpp src/StandaloneAudio.cpp src/ui/About.cpp src/ui/BuildInfo.cpp src/ui/Format.cpp src/ui/Instruments.cpp src/ui/Layout.cpp src/ui/MicMap.cpp src/ui/Spectro.cpp src/ui/Strip.cpp src/ui/StripInteraction.cpp src/ui/Viz.cpp src/ui/Tooltip.cpp src/ui/UiState.cpp src/SharedRegion.cpp src/ui/TelemetryClient.cpp src/ui/CurveEdit.cpp src/ui/FeltEditor.cpp src/ui/NoteHistory.cpp src/MidiMap.cpp src/MidiRoute.cpp src/Morph.cpp src/FileWatch.cpp src/Presets.cpp src/Paths.cpp src/Numbers.cpp src/StateVersion.cpp src/ui/GlCheck.cpp)
target_include_directories(pe_common PUBLIC src src/ui)
# the shipped full presets, each plugin's (presets/dussek/*.dussek-preset, presets/weidinger/*.weidinger-preset),
# embedded as text: Presets.cpp parses them (the files name instruments, which the code may not)
foreach(p ${PE_PLUGINS})
  file(GLOB PE_PRESET_FILES CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/presets/${p}/*.${p}-preset)
  list(SORT PE_PRESET_FILES)
  set(PE_PRESET_TEXTS "")
  foreach(f ${PE_PRESET_FILES})
    file(READ ${f} text)
    string(APPEND PE_PRESET_TEXTS "R\"pepreset(${text})pepreset\",\n")
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${f})
  endforeach()
  file(CONFIGURE OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/generated/ShippedPresets-${p}.inc CONTENT "${PE_PRESET_TEXTS}" @ONLY)
endforeach()
target_include_directories(pe_common PRIVATE ${CMAKE_CURRENT_BINARY_DIR}/generated)
# the version and the commit for the About tab (BuildInfo.cpp): the commit read at every build
# (cmake/BuildCommit.cmake; "unknown" without git or outside a checkout)
find_package(Git QUIET)
set(PE_COMMIT_HEADER ${CMAKE_CURRENT_BINARY_DIR}/generated/BuildCommit.h)
add_custom_target(pe_build_commit
  COMMAND ${CMAKE_COMMAND} "-DROOT=${CMAKE_CURRENT_SOURCE_DIR}/.." "-DGIT=${GIT_EXECUTABLE}" "-DOUT=${PE_COMMIT_HEADER}"
          -P ${CMAKE_CURRENT_SOURCE_DIR}/cmake/BuildCommit.cmake
  BYPRODUCTS ${PE_COMMIT_HEADER}
  VERBATIM)
add_dependencies(pe_common pe_build_commit)
set_source_files_properties(src/ui/BuildInfo.cpp PROPERTIES COMPILE_DEFINITIONS
  "PE_VERSION_MAJOR=${PROJECT_VERSION_MAJOR};PE_VERSION_MINOR=${PROJECT_VERSION_MINOR};PE_VERSION_PATCH=${PROJECT_VERSION_PATCH}")
target_link_libraries(pe_common PUBLIC engine)
target_link_libraries(pe_common PUBLIC Threads::Threads ${CMAKE_DL_LIBS} m)
set_target_properties(pe_common PROPERTIES POSITION_INDEPENDENT_CODE ON)
if(RT_LIB)
    target_link_libraries(pe_common PUBLIC ${RT_LIB}) # glibc before 2.34 keeps shm_open in librt
endif()
foreach(p ${PE_PLUGINS})
  target_link_libraries(${p} PUBLIC pe_common)
endforeach()

enable_testing()
# the Copy CLI line for named controls, for site/render.py (not shipped: outside bin/)
add_executable(settings_line tools/settings_line.cpp)
target_link_libraries(settings_line PRIVATE pe_common)

add_executable(ui_tests tests/ui_tests.cpp)
target_include_directories(ui_tests PRIVATE tests)
target_link_libraries(ui_tests PRIVATE pe_common)
target_compile_definitions(ui_tests PRIVATE PE_INSTRUMENTS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../instruments")
target_compile_definitions(ui_tests PRIVATE PE_TESTS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/tests")
add_test(NAME ui_tests COMMAND ui_tests)

foreach(p ${PE_PLUGINS})
  target_link_libraries(${p}-ui PUBLIC pe_common)
endforeach()
