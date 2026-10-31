# Plugin wrapper: DPF (C++) around the Rust engine (engine/, static library, C ABI).
#   just plugin   (fetches DPF at its pinned commit, configures plugin/build, builds)
cmake_minimum_required(VERSION 3.22)
# macOS: the oldest release the builds run on (libc++'s std::to_chars for floats: 13.3)
set(CMAKE_OSX_DEPLOYMENT_TARGET "13.3" CACHE STRING "the oldest macOS the builds run on")
# the plugin's version: the hosts' (getVersion) and the About tab's
project(dussek-plugin VERSION 0.3.0 LANGUAGES C CXX)

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

# the engine, built by cargo; when it is missing, a silent dummy engine
# (src/dummy) keeps everything buildable: the UI runs, the sound is silence
if(EXISTS ${CMAKE_CURRENT_SOURCE_DIR}/../engine/Cargo.toml)
  find_program(CARGO cargo REQUIRED)
  set(ENGINE_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../engine)
  set(ENGINE_LIB ${ENGINE_DIR}/target/release/${CMAKE_STATIC_LIBRARY_PREFIX}dussek${CMAKE_STATIC_LIBRARY_SUFFIX})
  add_custom_target(engine_cargo
    COMMAND ${CMAKE_COMMAND} -E env MACOSX_DEPLOYMENT_TARGET=${CMAKE_OSX_DEPLOYMENT_TARGET}
            ${CARGO} build --release --lib --manifest-path ${ENGINE_DIR}/Cargo.toml
    BYPRODUCTS ${ENGINE_LIB}
    USES_TERMINAL
    COMMENT "cargo build (engine)")
  add_library(engine STATIC IMPORTED)
  set_target_properties(engine PROPERTIES IMPORTED_LOCATION ${ENGINE_LIB})
  if(WIN32)
    # what Rust's standard library links on Windows (cargo rustc -- --print native-static-libs)
    set_property(TARGET engine PROPERTY INTERFACE_LINK_LIBRARIES
      kernel32 advapi32 ntdll userenv ws2_32 bcrypt dbghelp synchronization)
  endif()
  add_dependencies(engine engine_cargo)
  set(PE_ENGINE_LIB engine)
else()
  message(STATUS "engine/ missing: using the silent dummy engine (UI only, no sound)")
  add_library(pe_dummy STATIC src/dummy/DummyEngine.cpp)
  target_include_directories(pe_dummy PUBLIC src)
  set_target_properties(pe_dummy PROPERTIES POSITION_INDEPENDENT_CODE ON)
  set(PE_ENGINE_LIB pe_dummy)
endif()
find_package(Threads REQUIRED)
find_library(RT_LIB rt)

dpf_add_plugin(dussek
  TARGETS lv2 vst3 clap jack
  UI_TYPE opengl
  USE_FILE_BROWSER
  FILES_DSP src/PianoPlugin.cpp
  FILES_UI src/ui/PianoUI.cpp src/ui/Widgets.cpp src/ui/Panel.cpp src/ui/DiagView.cpp src/ui/DiagPanel.cpp src/ui/VizPanel.cpp src/ui/MicView.cpp)

target_include_directories(dussek PUBLIC src)
target_link_libraries(dussek PUBLIC Threads::Threads ${CMAKE_DL_LIBS} m)
# the standalone's `render` command (midi2wav's code through pe_cli), before DPF's main
target_sources(dussek-jack PRIVATE src/StandaloneCli.cpp)

# pure logic shared by the DSP and the UI, and its tests (no window)
add_library(pe_common STATIC src/Catalog.cpp src/Params.cpp src/EngineValues.cpp src/PedalMixer.cpp src/ui/About.cpp src/ui/BuildInfo.cpp src/ui/Format.cpp src/ui/Instruments.cpp src/ui/Layout.cpp src/ui/MicMap.cpp src/ui/Spectro.cpp src/ui/Strip.cpp src/ui/StripInteraction.cpp src/ui/Viz.cpp src/ui/Tooltip.cpp src/ui/UiState.cpp src/SharedRegion.cpp src/ui/TelemetryClient.cpp src/ui/CurveEdit.cpp src/ui/FeltEditor.cpp src/ui/NoteHistory.cpp src/MidiMap.cpp src/MidiRoute.cpp src/Morph.cpp src/FileWatch.cpp src/Presets.cpp src/Paths.cpp src/Numbers.cpp src/StateVersion.cpp src/ui/GlCheck.cpp)
target_include_directories(pe_common PUBLIC src src/ui)
# the shipped full presets (presets/*.dussek-preset), embedded as text: Presets.cpp parses them (the files
# name instruments, which the code may not)
file(GLOB PE_PRESET_FILES CONFIGURE_DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/presets/*.dussek-preset)
list(SORT PE_PRESET_FILES)
set(PE_PRESET_TEXTS "")
foreach(f ${PE_PRESET_FILES})
  file(READ ${f} text)
  string(APPEND PE_PRESET_TEXTS "R\"pepreset(${text})pepreset\",\n")
  set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${f})
endforeach()
file(CONFIGURE OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/generated/ShippedPresets.inc CONTENT "${PE_PRESET_TEXTS}" @ONLY)
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
target_link_libraries(pe_common PUBLIC ${PE_ENGINE_LIB})
target_link_libraries(pe_common PUBLIC Threads::Threads ${CMAKE_DL_LIBS} m)
set_target_properties(pe_common PROPERTIES POSITION_INDEPENDENT_CODE ON)
if(RT_LIB)
    target_link_libraries(pe_common PUBLIC ${RT_LIB}) # glibc before 2.34 keeps shm_open in librt
endif()
target_link_libraries(dussek PUBLIC pe_common)

enable_testing()
add_executable(ui_tests tests/ui_tests.cpp)
target_include_directories(ui_tests PRIVATE tests)
target_link_libraries(ui_tests PRIVATE pe_common)
target_compile_definitions(ui_tests PRIVATE PE_INSTRUMENTS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/../instruments")
target_compile_definitions(ui_tests PRIVATE PE_TESTS_DIR="${CMAKE_CURRENT_SOURCE_DIR}/tests")
add_test(NAME ui_tests COMMAND ui_tests)

target_link_libraries(dussek-ui PUBLIC pe_common)
