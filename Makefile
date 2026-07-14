# Copyright 2025 yu-iskw
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Set up the workspace
.PHONY: setup
setup:
	bash ./dev/setup.sh

# Check all the coding style.
.PHONY: lint
lint:
	bash ./dev/lint.sh

# Format source codes
.PHONY: format
format:
	bash ./dev/fmt.sh

# Run the unit tests.
.PHONY: test
test:
	bash ./dev/test.sh

# Run local CodeQL analysis.
.PHONY: codeql
codeql:
	bash ./dev/codeql.sh

# Build the package
.PHONY: build
build:
	bash ./dev/build.sh

# Clean the environment
.PHONY: clean
clean:
	bash ./dev/clean.sh

all: clean lint test build
