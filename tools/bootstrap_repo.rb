#!/usr/bin/env ruby
# frozen_string_literal: true

require "fileutils"
require "pathname"

root = Pathname.new(__dir__).parent
recipe = root.join("recipes", "hello_redux", "recipe.toml")
system("ruby", root.join("tools", "redux_recipe_build.rb").to_s, recipe.to_s, exception: true)
puts "Sample package ready from recipe: #{recipe}"
