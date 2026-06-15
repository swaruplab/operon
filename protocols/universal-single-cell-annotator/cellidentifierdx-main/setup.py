from setuptools import setup, find_packages

setup(
    name='cellidentifierdx',
    version='0.1',
    packages=find_packages(),
    author='mdbabumia',
    author_email='md.babu.mia@mssm.edu',
    description='A package to identify cell types using Bayesian scoring',
    long_description=open('README.md').read(),
    long_description_content_type='text/markdown',
    url='https://github.com/yourusername/cellidentifierdx',
    classifiers=[
        'Development Status :: 3 - Alpha',
        'License :: OSI Approved :: MIT License',
        'Programming Language :: Python :: 3.6',
        'Programming Language :: Python :: 3.7',
        'Programming Language :: Python :: 3.8',
        'Programming Language :: Python :: 3.10',
    ],
    python_requires='>=3.6',
)
