#!/bin/bash
# Check for banned patterns in UI components

failed=0

# overflow-hidden creates a scroll container that blocks trackpad scroll propagation
if grep -rn "overflow-hidden" --include='*.rs' bae-ui/src/components/ 2>/dev/null | grep -v "//"; then
    echo "Found overflow-hidden in UI components. Use overflow-clip instead."
    failed=1
fi

# Link underlines are banned — use TextLink component instead
if grep -rn "underline" --include='*.rs' bae-ui/src/components/ 2>/dev/null | grep -v "//"; then
    echo "Link underline not allowed. Use TextLink, which does not have underline."
    failed=1
fi

exit $failed
