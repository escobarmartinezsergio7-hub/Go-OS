# Install script for directory: /Users/mac/Documents/New project/kernel/third_party/litehtml/upstream

# Set the install prefix
if(NOT DEFINED CMAKE_INSTALL_PREFIX)
  set(CMAKE_INSTALL_PREFIX "/usr/local")
endif()
string(REGEX REPLACE "/$" "" CMAKE_INSTALL_PREFIX "${CMAKE_INSTALL_PREFIX}")

# Set the install configuration name.
if(NOT DEFINED CMAKE_INSTALL_CONFIG_NAME)
  if(BUILD_TYPE)
    string(REGEX REPLACE "^[^A-Za-z0-9_]+" ""
           CMAKE_INSTALL_CONFIG_NAME "${BUILD_TYPE}")
  else()
    set(CMAKE_INSTALL_CONFIG_NAME "")
  endif()
  message(STATUS "Install configuration: \"${CMAKE_INSTALL_CONFIG_NAME}\"")
endif()

# Set the component getting installed.
if(NOT CMAKE_INSTALL_COMPONENT)
  if(COMPONENT)
    message(STATUS "Install component: \"${COMPONENT}\"")
    set(CMAKE_INSTALL_COMPONENT "${COMPONENT}")
  else()
    set(CMAKE_INSTALL_COMPONENT)
  endif()
endif()

# Is this installation the result of a crosscompile?
if(NOT DEFINED CMAKE_CROSSCOMPILING)
  set(CMAKE_CROSSCOMPILING "TRUE")
endif()

# Set path to fallback-tool for dependency-resolution.
if(NOT DEFINED CMAKE_OBJDUMP)
  set(CMAKE_OBJDUMP "/opt/homebrew/bin/x86_64-w64-mingw32-objdump")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "libraries" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib" TYPE STATIC_LIBRARY FILES "/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/liblitehtml.a")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/include/litehtml" TYPE FILE FILES
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/background.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/borders.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/codepoint.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_length.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_margins.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_offsets.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_position.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_selector.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_parser.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_tokenizer.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/document.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/document_container.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_anchor.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_base.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_before_after.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_body.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_break.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_cdata.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_comment.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_div.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_font.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_image.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_link.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_para.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_script.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_space.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_style.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_table.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_td.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_text.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_title.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/el_tr.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/element.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/encodings.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/html.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/html_tag.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/html_microsyntaxes.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/iterators.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/media_query.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/os_types.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/style.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/stylesheet.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/table.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/tstring_view.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/types.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/url.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/url_path.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/utf8_strings.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/web_color.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/num_cvt.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/css_properties.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/line_box.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_item.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_flex.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_image.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_inline.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_table.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_inline_context.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_block_context.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/render_block.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/master_css.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/string_id.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/formatting_context.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/flex_item.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/flex_line.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/gradient.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/font_description.h"
    "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/include/litehtml/scroll_view.h"
    )
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml" TYPE FILE FILES "/Users/mac/Documents/New project/kernel/third_party/litehtml/upstream/cmake/litehtmlConfig.cmake")
endif()

if(CMAKE_INSTALL_COMPONENT STREQUAL "Unspecified" OR NOT CMAKE_INSTALL_COMPONENT)
  if(EXISTS "$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml/litehtmlTargets.cmake")
    file(DIFFERENT _cmake_export_file_changed FILES
         "$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml/litehtmlTargets.cmake"
         "/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/CMakeFiles/Export/1858d3296707c77b4f85418fd0121701/litehtmlTargets.cmake")
    if(_cmake_export_file_changed)
      file(GLOB _cmake_old_config_files "$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml/litehtmlTargets-*.cmake")
      if(_cmake_old_config_files)
        string(REPLACE ";" ", " _cmake_old_config_files_text "${_cmake_old_config_files}")
        message(STATUS "Old export file \"$ENV{DESTDIR}${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml/litehtmlTargets.cmake\" will be replaced.  Removing files [${_cmake_old_config_files_text}].")
        unset(_cmake_old_config_files_text)
        file(REMOVE ${_cmake_old_config_files})
      endif()
      unset(_cmake_old_config_files)
    endif()
    unset(_cmake_export_file_changed)
  endif()
  file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml" TYPE FILE FILES "/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/CMakeFiles/Export/1858d3296707c77b4f85418fd0121701/litehtmlTargets.cmake")
  if(CMAKE_INSTALL_CONFIG_NAME MATCHES "^()$")
    file(INSTALL DESTINATION "${CMAKE_INSTALL_PREFIX}/lib/cmake/litehtml" TYPE FILE FILES "/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/CMakeFiles/Export/1858d3296707c77b4f85418fd0121701/litehtmlTargets-noconfig.cmake")
  endif()
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  # Include the install script for each subdirectory.
  include("/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/src/gumbo/cmake_install.cmake")

endif()

string(REPLACE ";" "\n" CMAKE_INSTALL_MANIFEST_CONTENT
       "${CMAKE_INSTALL_MANIFEST_FILES}")
if(CMAKE_INSTALL_LOCAL_ONLY)
  file(WRITE "/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/install_local_manifest.txt"
     "${CMAKE_INSTALL_MANIFEST_CONTENT}")
endif()
if(CMAKE_INSTALL_COMPONENT)
  if(CMAKE_INSTALL_COMPONENT MATCHES "^[a-zA-Z0-9_.+-]+$")
    set(CMAKE_INSTALL_MANIFEST "install_manifest_${CMAKE_INSTALL_COMPONENT}.txt")
  else()
    string(MD5 CMAKE_INST_COMP_HASH "${CMAKE_INSTALL_COMPONENT}")
    set(CMAKE_INSTALL_MANIFEST "install_manifest_${CMAKE_INST_COMP_HASH}.txt")
    unset(CMAKE_INST_COMP_HASH)
  endif()
else()
  set(CMAKE_INSTALL_MANIFEST "install_manifest.txt")
endif()

if(NOT CMAKE_INSTALL_LOCAL_ONLY)
  file(WRITE "/Users/mac/Documents/New project/kernel/third_party/litehtml/build-win64/${CMAKE_INSTALL_MANIFEST}"
     "${CMAKE_INSTALL_MANIFEST_CONTENT}")
endif()
