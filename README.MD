
# Tool to sanitize Pants dependencies.

This tool works with a report from the following command (run from the root folder of an inspected project):

    ./pants -q dep-usage.jvm --no-summary src/scala/:: > deps.json
     
Make this report before using this tool, it may take a few minutes!

## How to build

Binary (cargo required - [how to install rust](https://www.rust-lang.org/tools/install))

    cargo build --release
    
Docker (docker required)(may take a few minutes)

    docker build -t dep-sanitizer .
    
    
## How to use

Run this tool from the root of your Pants project. If the report file isn't in the project root, 
specify report file via `--report_file` e.g.

    dep-sanitizer --report_file=/tmp/deps.json unused show 

For getting help

    dep-sanitizer help    
    
For showing all unused dependencies

    dep-sanitizer --prefix=src/ unused show 
    
For remove all unused dependencies

    dep-sanitizer unused fix
    
For showing all undeclared but used transitively modules dependencies

    dep-sanitizer undeclared show 
    
For adding all undeclared dependencies to corresponded BUILD files

    dep-sanitizer undeclared fix
    
Use with docker 

    docker run -v ${PWD}/../your-repo:/project/ dep-sanitizer unused show 
  
Note that, we mount 'your-repo' project into a container as 'project' folder. This folder 
should contain 'deps.json' file as well.   
    
## Skipping dependency

Sometimes we can get false positives and try to remove correct dependencies (very rare).
If it happens to you, add this comment to the dependency line:

        dependencies=[
            'src/scala/whisk/util/converters', #skip-sanitize
            ...
        ]
