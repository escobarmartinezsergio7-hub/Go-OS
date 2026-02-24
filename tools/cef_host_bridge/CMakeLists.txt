cmake_minimum_required(VERSION 3.16)
project(cef_host_bridge LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT DEFINED CEF_ROOT OR CEF_ROOT STREQUAL "")
  message(FATAL_ERROR "CEF_ROOT no definido. Ejemplo: -DCEF_ROOT=/ruta/a/cef_binary")
endif()

set(CEF_INCLUDE_DIR "${CEF_ROOT}")

add_executable(cef_host_bridge
  src/main.cpp
)

target_include_directories(cef_host_bridge PRIVATE "${CEF_INCLUDE_DIR}")

if(APPLE)
  find_library(CEF_FRAMEWORK
    NAMES Chromium Embedded Framework
    PATHS
      "${CEF_ROOT}/Release"
      "${CEF_ROOT}"
    NO_DEFAULT_PATH
  )
  if(NOT CEF_FRAMEWORK)
    message(FATAL_ERROR "No se encontro Chromium Embedded Framework en CEF_ROOT.")
  endif()

  target_link_libraries(cef_host_bridge PRIVATE
    "${CEF_FRAMEWORK}"
    "-framework AppKit"
    "-framework Cocoa"
    "-framework Foundation"
  )
elseif(WIN32)
  find_library(CEF_LIBRARY
    NAMES libcef
    PATHS
      "${CEF_ROOT}/Release"
      "${CEF_ROOT}"
    NO_DEFAULT_PATH
  )
  if(NOT CEF_LIBRARY)
    message(FATAL_ERROR "No se encontro libcef.lib en CEF_ROOT.")
  endif()
  target_link_libraries(cef_host_bridge PRIVATE "${CEF_LIBRARY}")
elseif(UNIX)
  set(CEF_LIBRARY "${CEF_ROOT}/Release/libcef.so")
  if(NOT EXISTS "${CEF_LIBRARY}")
    message(FATAL_ERROR "No se encontro ${CEF_LIBRARY}.")
  endif()

  target_link_libraries(cef_host_bridge PRIVATE
    "${CEF_LIBRARY}"
    pthread
    dl
    X11
  )

  set_target_properties(cef_host_bridge PROPERTIES
    BUILD_RPATH "${CEF_ROOT}/Release"
    INSTALL_RPATH "${CEF_ROOT}/Release"
  )
endif()

message(STATUS "CEF_ROOT=${CEF_ROOT}")
